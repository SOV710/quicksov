// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use nix::libc;
use tokio::process::Command;
use tokio::sync::watch;
use tracing::{info, warn};

use super::config::{WallpaperCfg, DEFAULT_RENDERER_BINARY};
use super::model::RendererRuntime;

pub(super) async fn supervise_renderer(
    cfg: WallpaperCfg,
    state_tx: watch::Sender<RendererRuntime>,
) {
    let binary = renderer_binary_path();

    loop {
        let mut command = Command::new(&binary);
        command.env("QSOV_SOCKET", &cfg.socket_path);
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }

        match command.spawn() {
            Ok(mut child) => {
                let pid = child.id().unwrap_or_default();
                info!(pid, path = %binary.display(), "spawned wallpaper renderer");
                state_tx.send_replace(RendererRuntime::running(pid));

                match child.wait().await {
                    Ok(status) if status.success() => {
                        warn!("wallpaper renderer exited cleanly; restarting");
                        state_tx.send_replace(RendererRuntime::error(
                            "renderer exited unexpectedly".to_string(),
                        ));
                    }
                    Ok(status) => {
                        warn!(status = %status, "wallpaper renderer exited with failure");
                        state_tx.send_replace(RendererRuntime::error(format!(
                            "renderer exited with status {status}"
                        )));
                    }
                    Err(err) => {
                        warn!(error = %err, "failed to wait for wallpaper renderer");
                        state_tx.send_replace(RendererRuntime::error(format!(
                            "failed to wait for renderer: {err}"
                        )));
                    }
                }
            }
            Err(err) => {
                warn!(
                    error = %err,
                    path = %binary.display(),
                    "failed to spawn wallpaper renderer"
                );
                state_tx.send_replace(RendererRuntime::error(format!(
                    "failed to spawn renderer: {err}"
                )));
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn renderer_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("QSOV_WALLPAPER_NATIVE") {
        return PathBuf::from(path);
    }

    let mut candidates = Vec::<PathBuf>::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(DEFAULT_RENDERER_BINARY));
            if let Some(target_dir) = dir.parent() {
                if let Some(repo_root) = target_dir.parent() {
                    candidates.push(
                        repo_root
                            .join(".build")
                            .join("native")
                            .join("wallpaper_native")
                            .join(DEFAULT_RENDERER_BINARY),
                    );
                }
            }
        }
    }

    candidates.push(PathBuf::from(DEFAULT_RENDERER_BINARY));

    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(DEFAULT_RENDERER_BINARY)
}
