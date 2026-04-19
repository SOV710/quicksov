#![deny(warnings)]

// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};
use std::time::Duration;

use nix::sys::signal::Signal;
use thiserror::Error;
use tokio::process::Command;
use tracing::{info, warn};

#[derive(Debug, Error)]
enum MainError {
    #[error("failed to initialise tracing: {0}")]
    Tracing(String),
    #[error("failed to set parent-death signal: {0}")]
    Pdeathsig(#[from] nix::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), MainError> {
    init_tracing()?;
    nix::sys::prctl::set_pdeathsig(Signal::SIGTERM)?;

    let shell_path = wallpaper_shell_path();
    let import_path = wallpaper_qml_import_path(&shell_path);

    info!(path = %shell_path.display(), "wallpaper renderer supervisor started");

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    loop {
        let mut command = Command::new("qs");
        command.arg("-p").arg(&shell_path);
        command.env("QSG_RHI_BACKEND", "opengl");
        command.env("LC_NUMERIC", "C");
        if let Some(value) = &import_path {
            command.env("QML_IMPORT_PATH", value);
        }
        if let Ok(socket) = std::env::var("QSOV_SOCKET") {
            command.env("QSOV_SOCKET", socket);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                warn!(error = %err, "failed to spawn wallpaper quickshell process");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        let pid = child.id().unwrap_or_default();
        info!(pid, "spawned wallpaper quickshell process");

        let should_exit = tokio::select! {
            _ = sigterm.recv() => {
                info!("received SIGTERM, shutting down wallpaper quickshell process");
                if let Err(err) = child.start_kill() {
                    warn!(error = %err, "failed to terminate wallpaper quickshell child");
                }
                true
            }
            result = child.wait() => {
                match result {
                    Ok(status) if status.success() => {
                        warn!("wallpaper quickshell child exited cleanly; restarting");
                    }
                    Ok(status) => {
                        warn!(status = %status, "wallpaper quickshell child exited with failure");
                    }
                    Err(err) => {
                        warn!(error = %err, "failed while waiting for wallpaper quickshell child");
                    }
                }
                false
            }
        };

        if should_exit {
            break;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    info!("wallpaper renderer supervisor stopped");
    Ok(())
}

fn init_tracing() -> Result<(), MainError> {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_new("info").map_err(|err| MainError::Tracing(err.to_string()))?;
    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
    Ok(())
}

fn wallpaper_shell_path() -> PathBuf {
    if let Ok(path) = std::env::var("QSOV_WALLPAPER_SHELL") {
        return PathBuf::from(path);
    }

    let config_root = if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg_config)
    } else if let Some(home) = dirs::home_dir() {
        home.join(".config")
    } else {
        PathBuf::from("$HOME/.config")
    };

    config_root
        .join("quickshell")
        .join("quicksov")
        .join("wallpaper-shell.qml")
}

fn wallpaper_qml_import_path(shell_path: &Path) -> Option<String> {
    let mut paths = Vec::<String>::new();

    if let Ok(existing) = std::env::var("QML_IMPORT_PATH") {
        if !existing.is_empty() {
            paths.push(existing);
        }
    }

    if let Some(shell_dir) = shell_path.parent() {
        paths.insert(0, shell_dir.to_string_lossy().into_owned());
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(target_dir) = exe.parent().and_then(Path::parent) {
            let maybe_build_qml = target_dir
                .parent()
                .map(|repo| repo.join(".build").join("qml"));
            if let Some(build_qml) = maybe_build_qml {
                if build_qml.exists() {
                    paths.insert(0, build_qml.to_string_lossy().into_owned());
                }
            }
        }
    }

    if paths.is_empty() {
        None
    } else {
        Some(paths.join(":"))
    }
}
