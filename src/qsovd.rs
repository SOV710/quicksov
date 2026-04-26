// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use thiserror::Error;
use tracing::{info, warn};

use crate::{config, ipc, platform, services};

/// Aggregates all errors that can occur during daemon startup.
#[derive(Debug, Error)]
pub enum MainError {
    #[error("configuration error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("tracing initialisation failed: {0}")]
    TracingInit(String),
    #[error("platform setup failed: {0}")]
    Platform(#[from] platform::linux::PlatformError),
    #[error("IPC error: {0}")]
    Ipc(#[from] ipc::IpcError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("another qsovd instance is already running on the socket")]
    AlreadyRunning,
}

pub fn main() -> Result<(), MainError> {
    let started_at = std::time::Instant::now();
    let (config, used_defaults) = config::load_with_info()?;

    init_tracing(&config.daemon.log_level)?;

    if used_defaults {
        warn!("config file not found, using built-in defaults");
    }

    platform::linux::set_parent_death_signal()?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main(config, started_at))
}

async fn async_main(
    config: config::Config,
    started_at: std::time::Instant,
) -> Result<(), MainError> {
    let socket_path = PathBuf::from(&config.daemon.socket_path);
    let services = services::start_services(&config, started_at).await;
    let router = ipc::router::Router::new(services);
    let capabilities = router.capabilities();

    let listener = match ipc::transport::bind(&socket_path).await {
        Ok(listener) => listener,
        Err(ipc::transport::TransportError::AlreadyRunning) => {
            return Err(MainError::AlreadyRunning);
        }
        Err(error) => return Err(ipc::IpcError::from(error).into()),
    };

    info!(path = %socket_path.display(), "listening on Unix socket");

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut session_set: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
    let mut session_counter: u64 = 0;

    loop {
        tokio::select! {
            _ = sigterm.recv() => {
                info!("received SIGTERM, beginning graceful shutdown");
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let sid = session_counter;
                        session_counter += 1;
                        let router = router.clone();
                        let caps = capabilities.clone();
                        session_set.spawn(ipc::session::run_session(stream, sid, router, caps));
                        tracing::info!(session_id = sid, "accepted new connection");
                    }
                    Err(error) => {
                        tracing::error!(error = %error, "accept error");
                    }
                }
            }
        }
    }

    info!("waiting for active sessions to finish (up to 5 s)");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    loop {
        if session_set.is_empty() {
            break;
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            warn!("shutdown timeout reached; aborting remaining sessions");
            session_set.abort_all();
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(remaining) => {
                warn!("shutdown timeout reached; aborting remaining sessions");
                session_set.abort_all();
                break;
            }
            _ = session_set.join_next() => {}
        }
    }

    if let Err(error) = std::fs::remove_file(&socket_path) {
        warn!(error = %error, "failed to remove socket file during shutdown");
    }

    info!("shutdown complete");
    Ok(())
}

fn init_tracing(log_level: &str) -> Result<(), MainError> {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter =
        EnvFilter::try_new(log_level).map_err(|error| MainError::TracingInit(error.to_string()))?;

    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    Ok(())
}
