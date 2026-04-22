// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io;
use std::time::Duration;

use nix::libc;
use tokio::process::Command;
use tokio::sync::watch;
use tracing::{info, warn};

use super::config::{resolve_renderer_binary_path, WallpaperCfg};
use super::model::RendererRuntime;
use crate::wallpaper_contract::{
    WALLPAPER_RENDERER_CLIENT_NAME, WALLPAPER_RENDERER_CLIENT_VERSION,
};

pub(super) async fn supervise_renderer(
    cfg: WallpaperCfg,
    state_tx: watch::Sender<RendererRuntime>,
) {
    let binary = resolve_renderer_binary_path();

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
                info!(
                    pid,
                    path = %binary.display(),
                    client = WALLPAPER_RENDERER_CLIENT_NAME,
                    client_version = WALLPAPER_RENDERER_CLIENT_VERSION,
                    "spawned wallpaper renderer"
                );
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
