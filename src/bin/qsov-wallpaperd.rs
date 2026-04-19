#![deny(warnings)]

// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::unix::process::CommandExt;
use std::path::PathBuf;

use nix::sys::signal::Signal;
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
enum MainError {
    #[error("failed to initialise tracing: {0}")]
    Tracing(String),
    #[error("failed to set parent-death signal: {0}")]
    Pdeathsig(#[from] nix::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("native wallpaper renderer binary not found")]
    NativeBinaryMissing,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), MainError> {
    init_tracing()?;
    nix::sys::prctl::set_pdeathsig(Signal::SIGTERM)?;

    let binary = native_renderer_path()?;

    info!(path = %binary.display(), "execing native wallpaper renderer");

    let mut command = std::process::Command::new(&binary);
    if let Ok(socket) = std::env::var("QSOV_SOCKET") {
        command.env("QSOV_SOCKET", socket);
    }

    Err(command.exec().into())
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

fn native_renderer_path() -> Result<PathBuf, MainError> {
    if let Ok(path) = std::env::var("QSOV_WALLPAPER_NATIVE") {
        return Ok(PathBuf::from(path));
    }

    let mut candidates = Vec::<PathBuf>::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("qsov-wallpaper-native"));
            if let Some(target_dir) = dir.parent() {
                if let Some(repo_root) = target_dir.parent() {
                    candidates.push(
                        repo_root
                            .join(".build")
                            .join("native")
                            .join("wallpaper_native")
                            .join("qsov-wallpaper-native"),
                    );
                }
            }
        }
    }

    candidates.push(PathBuf::from("qsov-wallpaper-native"));

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(MainError::NativeBinaryMissing)
}
