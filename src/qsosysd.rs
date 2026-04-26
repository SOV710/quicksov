// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::fd::FromRawFd;
use std::os::unix::fs::{chown, PermissionsExt};
use std::path::Path;

use thiserror::Error;
use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::paths::{QSOSYSD_GROUP_NAME, QSOSYSD_SOCKET_PATH};
use crate::ipc::{codec, transport};
use crate::services::battery::helper_protocol::{HelperErrorKind, HelperRequest, HelperResponse};
use crate::services::battery::power_profile::{
    write_platform_profile, PlatformProfilePaths, PlatformProfileWriteError, ProductPowerProfile,
};

const SYSTEMD_LISTEN_FDS_START: i32 = 3;

#[derive(Debug, Error)]
pub enum MainError {
    #[error("tracing initialisation failed: {0}")]
    TracingInit(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("transport error: {0}")]
    Transport(#[from] transport::TransportError),
    #[error("system setup failed: {0}")]
    Setup(String),
}

pub fn main() -> Result<(), MainError> {
    init_tracing()?;

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}

async fn async_main() -> Result<(), MainError> {
    let listener = acquire_listener().await?;
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let paths = PlatformProfilePaths::default();

    info!(socket = QSOSYSD_SOCKET_PATH, "qsosysd ready");

    loop {
        tokio::select! {
            _ = sigterm.recv() => {
                info!("qsosysd received SIGTERM");
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        let paths = paths.clone();
                        tokio::spawn(async move {
                            if let Err(error) = handle_client(stream, &paths).await {
                                warn!(error = %error, "qsosysd client request failed");
                            }
                        });
                    }
                    Err(error) => {
                        warn!(error = %error, "qsosysd accept failed");
                    }
                }
            }
        }
    }

    Ok(())
}

fn init_tracing() -> Result<(), MainError> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
    Ok(())
}

async fn acquire_listener() -> Result<UnixListener, MainError> {
    if let Some(listener) = listener_from_systemd_socket()? {
        return Ok(listener);
    }

    let path = Path::new(QSOSYSD_SOCKET_PATH);
    let listener = transport::bind(path).await?;
    configure_socket_permissions(path)?;
    Ok(listener)
}

fn listener_from_systemd_socket() -> Result<Option<UnixListener>, MainError> {
    let listen_pid = match std::env::var("LISTEN_PID") {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let Ok(pid) = listen_pid.parse::<u32>() else {
        return Ok(None);
    };
    if pid != std::process::id() {
        return Ok(None);
    }

    let listen_fds = std::env::var("LISTEN_FDS")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0);
    if listen_fds < 1 {
        return Ok(None);
    }

    let std_listener =
        unsafe { std::os::unix::net::UnixListener::from_raw_fd(SYSTEMD_LISTEN_FDS_START) };
    std_listener.set_nonblocking(true)?;
    UnixListener::from_std(std_listener)
        .map(Some)
        .map_err(MainError::Io)
}

fn configure_socket_permissions(path: &Path) -> Result<(), MainError> {
    let group = nix::unistd::Group::from_name(QSOSYSD_GROUP_NAME)
        .map_err(|error| MainError::Setup(error.to_string()))?
        .ok_or_else(|| MainError::Setup(format!("group {QSOSYSD_GROUP_NAME:?} not found")))?;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;
    chown(path, Some(0), Some(group.gid.as_raw()))
        .map_err(|error| MainError::Setup(error.to_string()))?;
    Ok(())
}

async fn handle_client(stream: UnixStream, paths: &PlatformProfilePaths) -> Result<(), MainError> {
    let (reader_half, mut writer_half) = stream.into_split();
    let mut reader = BufReader::new(reader_half);
    let Some(line) = transport::read_line(&mut reader).await? else {
        return Ok(());
    };
    let request: Result<HelperRequest, _> = codec::decode(line.trim_end_matches('\n'));
    let response = match request {
        Ok(request) => process_request(request, paths),
        Err(error) => HelperResponse::Error {
            kind: HelperErrorKind::InvalidRequest,
            message: error.to_string(),
        },
    };
    let encoded = codec::encode(&response).map_err(|error| MainError::Setup(error.to_string()))?;
    transport::write_line(&mut writer_half, &encoded).await?;
    Ok(())
}

fn process_request(request: HelperRequest, paths: &PlatformProfilePaths) -> HelperResponse {
    if request.action != HelperRequest::SET_PLATFORM_PROFILE_ACTION {
        return HelperResponse::Error {
            kind: HelperErrorKind::InvalidRequest,
            message: format!("unknown action: {}", request.action),
        };
    }

    let Some(profile) = ProductPowerProfile::from_action_str(&request.profile) else {
        return HelperResponse::Error {
            kind: HelperErrorKind::InvalidRequest,
            message: format!("invalid profile: {}", request.profile),
        };
    };

    match write_platform_profile(paths, profile) {
        Ok(raw_profile) => HelperResponse::Ok {
            profile: profile.as_str().to_string(),
            raw_profile,
        },
        Err(error) => HelperResponse::Error {
            kind: map_helper_error_kind(&error),
            message: error.to_string(),
        },
    }
}

fn map_helper_error_kind(error: &PlatformProfileWriteError) -> HelperErrorKind {
    match error {
        PlatformProfileWriteError::Unsupported => HelperErrorKind::Unsupported,
        PlatformProfileWriteError::PermissionDenied(_) => HelperErrorKind::PermissionDenied,
        PlatformProfileWriteError::BackendUnavailable(_) => HelperErrorKind::BackendUnavailable,
        PlatformProfileWriteError::WriteFailed(_) => HelperErrorKind::WriteFailed,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::services::battery::helper_protocol::{
        HelperErrorKind, HelperRequest, HelperResponse,
    };
    use crate::services::battery::power_profile::PlatformProfilePaths;

    use super::process_request;

    struct TestDir {
        path: std::path::PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "quicksov-qsosysd-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn paths(&self) -> PlatformProfilePaths {
            PlatformProfilePaths {
                profile_path: self.path.join("platform_profile"),
                choices_path: self.path.join("platform_profile_choices"),
                helper_socket_path: self.path.join("qsosysd.sock"),
            }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn rejects_unknown_action() {
        let dir = TestDir::new();
        let response = process_request(
            HelperRequest {
                action: "other".to_string(),
                profile: "balanced".to_string(),
            },
            &dir.paths(),
        );

        assert_eq!(
            response,
            HelperResponse::Error {
                kind: HelperErrorKind::InvalidRequest,
                message: "unknown action: other".to_string(),
            }
        );
    }

    #[test]
    fn rejects_invalid_profile_name() {
        let dir = TestDir::new();
        let response = process_request(
            HelperRequest {
                action: HelperRequest::SET_PLATFORM_PROFILE_ACTION.to_string(),
                profile: "turbo".to_string(),
            },
            &dir.paths(),
        );

        assert_eq!(
            response,
            HelperResponse::Error {
                kind: HelperErrorKind::InvalidRequest,
                message: "invalid profile: turbo".to_string(),
            }
        );
    }
}
