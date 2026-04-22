#![deny(warnings)]

// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

mod bus;
mod config;
mod ipc;
mod platform;
mod services;
mod session_env;
mod util;
mod wallpaper_contract;

use std::path::PathBuf;
use thiserror::Error;
use tracing::{info, warn};

/// Aggregates all errors that can occur during daemon startup.
#[derive(Debug, Error)]
enum MainError {
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

fn main() -> Result<(), MainError> {
    // Capture start instant as early as possible for uptime tracking.
    let started_at = std::time::Instant::now();

    // Load config synchronously (before the async runtime) so we can
    // initialise tracing with the correct log level from the first moment.
    let (config, used_defaults) = config::load_with_info()?;

    init_tracing(&config.daemon.log_level)?;

    if used_defaults {
        warn!("config file not found, using built-in defaults");
    }

    // Arrange for SIGTERM to be sent to this process when the parent (Niri) dies.
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

    // Start services according to the enabled list in config.
    let services = services::start_services(&config, started_at).await;
    let router = ipc::router::Router::new(services);
    let capabilities = router.capabilities();

    // Bind the UDS listener, handling stale-socket detection.
    let listener = match ipc::transport::bind(&socket_path).await {
        Ok(l) => l,
        Err(ipc::transport::TransportError::AlreadyRunning) => {
            return Err(MainError::AlreadyRunning);
        }
        Err(e) => return Err(ipc::IpcError::from(e).into()),
    };

    info!(path = %socket_path.display(), "listening on Unix socket");

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    let mut session_set: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
    let mut session_counter: u64 = 0;

    // -------------------------------------------------------------------------
    // Accept loop
    // -------------------------------------------------------------------------
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
                        let r = router.clone();
                        let caps = capabilities.clone();
                        session_set.spawn(ipc::session::run_session(stream, sid, r, caps));
                        tracing::info!(session_id = sid, "accepted new connection");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "accept error");
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // Graceful shutdown: wait up to 5 s for active sessions to drain
    // -------------------------------------------------------------------------
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

    // Remove the socket file so future daemon invocations don't see a stale entry.
    if let Err(e) = std::fs::remove_file(&socket_path) {
        warn!(error = %e, "failed to remove socket file during shutdown");
    }

    info!("shutdown complete");
    Ok(())
}

fn init_tracing(log_level: &str) -> Result<(), MainError> {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter =
        EnvFilter::try_new(log_level).map_err(|e| MainError::TracingInit(e.to_string()))?;

    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    Ok(())
}
