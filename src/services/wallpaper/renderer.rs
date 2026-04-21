// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::watch;
use tracing::{info, warn};

use super::config::{WallpaperCfg, DEFAULT_RENDERER_PROCESS};
use super::model::RendererRuntime;

pub(super) async fn supervise_renderer(
    cfg: WallpaperCfg,
    state_tx: watch::Sender<RendererRuntime>,
) {
    let binary = renderer_binary_path();

    loop {
        let mut command = Command::new(&binary);
        command.env("QSOV_SOCKET", &cfg.socket_path);

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
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(DEFAULT_RENDERER_PROCESS);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(DEFAULT_RENDERER_PROCESS)
}
