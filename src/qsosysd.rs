// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;
use tokio::io::AsyncWrite;
use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::paths::{QSOSYSD_SOCKET_ADDR_DISPLAY, QSOSYSD_SOCKET_ADDR_RAW};
use crate::ipc::{codec, transport};
use crate::services::battery::helper_protocol::{HelperErrorKind, HelperRequest, HelperResponse};
use crate::services::battery::power_profile::{
    write_platform_profile, PlatformProfilePaths, PlatformProfileWriteError, ProductPowerProfile,
};

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

    info!(socket = QSOSYSD_SOCKET_ADDR_DISPLAY, "qsosysd ready");

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
    UnixListener::bind(QSOSYSD_SOCKET_ADDR_RAW).map_err(MainError::Io)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PeerIdentity {
    uid: u32,
    pid: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthorizedCaller {
    Root(PeerIdentity),
    Daemon(PeerIdentity),
}

#[derive(Debug, Error, PartialEq, Eq)]
enum AuthError {
    #[error("failed to read peer credentials: {message}")]
    PeerCred { message: String },
    #[error("peer uid {uid} did not include a pid")]
    MissingPid { uid: u32 },
    #[error("failed to read /proc/{pid}/exe: {message}")]
    ReadExe { pid: i32, message: String },
    #[error("refusing deleted executable for pid {pid}: {exe}")]
    DeletedExe { pid: i32, exe: String },
    #[error("refusing executable basename for pid {pid}: expected qsovd, got {exe}")]
    InvalidExe { pid: i32, exe: String },
}

async fn handle_client(
    mut stream: UnixStream,
    paths: &PlatformProfilePaths,
) -> Result<(), MainError> {
    let caller = match authorize_stream(&stream) {
        Ok(caller) => caller,
        Err(error) => {
            let response = HelperResponse::Error {
                kind: HelperErrorKind::PermissionDenied,
                message: error.to_string(),
            };
            write_response(&mut stream, &response).await?;
            return Ok(());
        }
    };

    if let AuthorizedCaller::Root(identity) = caller {
        info!(uid = identity.uid, pid = ?identity.pid, "qsosysd authorized root caller");
    }

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
    write_response(&mut writer_half, &response).await?;
    Ok(())
}

fn authorize_stream(stream: &UnixStream) -> Result<AuthorizedCaller, AuthError> {
    let cred = stream.peer_cred().map_err(|error| AuthError::PeerCred {
        message: error.to_string(),
    })?;
    let identity = PeerIdentity {
        uid: cred.uid(),
        pid: cred.pid(),
    };
    authorize_peer_identity(identity, read_proc_exe_path)
}

fn authorize_peer_identity<F>(
    identity: PeerIdentity,
    read_exe: F,
) -> Result<AuthorizedCaller, AuthError>
where
    F: FnOnce(i32) -> io::Result<PathBuf>,
{
    if identity.uid == 0 {
        return Ok(AuthorizedCaller::Root(identity));
    }

    let pid = identity
        .pid
        .ok_or(AuthError::MissingPid { uid: identity.uid })?;
    let exe_path = read_exe(pid).map_err(|error| AuthError::ReadExe {
        pid,
        message: error.to_string(),
    })?;
    let exe_name = exe_path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| AuthError::InvalidExe {
            pid,
            exe: exe_path.display().to_string(),
        })?;

    if exe_name.ends_with(" (deleted)") {
        return Err(AuthError::DeletedExe {
            pid,
            exe: exe_name.to_string(),
        });
    }
    if exe_name != "qsovd" {
        return Err(AuthError::InvalidExe {
            pid,
            exe: exe_name.to_string(),
        });
    }

    Ok(AuthorizedCaller::Daemon(identity))
}

fn read_proc_exe_path(pid: i32) -> io::Result<PathBuf> {
    std::fs::read_link(Path::new("/proc").join(pid.to_string()).join("exe"))
}

async fn write_response<W>(writer: &mut W, response: &HelperResponse) -> Result<(), MainError>
where
    W: AsyncWrite + Unpin,
{
    let encoded = codec::encode(response).map_err(|error| MainError::Setup(error.to_string()))?;
    transport::write_line(writer, &encoded).await?;
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
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use tokio::io::BufReader;
    use tokio::net::{UnixListener, UnixStream};

    use crate::ipc::{codec, transport};
    use crate::services::battery::helper_protocol::{
        HelperErrorKind, HelperRequest, HelperResponse,
    };
    use crate::services::battery::power_profile::PlatformProfilePaths;

    use super::{
        authorize_peer_identity, process_request, write_response, AuthError, AuthorizedCaller,
        PeerIdentity,
    };

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
                helper_socket_addr: self.path.join("qsosysd.sock"),
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

    #[test]
    fn authorizes_root_without_proc_lookup() {
        let result = authorize_peer_identity(PeerIdentity { uid: 0, pid: None }, |_| {
            panic!("root callers should not hit /proc lookup")
        });

        assert_eq!(
            result,
            Ok(AuthorizedCaller::Root(PeerIdentity { uid: 0, pid: None }))
        );
    }

    #[test]
    fn authorizes_qsovd_basename() {
        let result = authorize_peer_identity(
            PeerIdentity {
                uid: 1000,
                pid: Some(42),
            },
            |_| Ok(PathBuf::from("/usr/bin/qsovd")),
        );

        assert_eq!(
            result,
            Ok(AuthorizedCaller::Daemon(PeerIdentity {
                uid: 1000,
                pid: Some(42),
            }))
        );
    }

    #[test]
    fn rejects_non_qsovd_basename() {
        let result = authorize_peer_identity(
            PeerIdentity {
                uid: 1000,
                pid: Some(42),
            },
            |_| Ok(PathBuf::from("/usr/bin/bash")),
        );

        assert_eq!(
            result,
            Err(AuthError::InvalidExe {
                pid: 42,
                exe: "bash".to_string(),
            })
        );
    }

    #[test]
    fn rejects_missing_pid_for_non_root() {
        let result = authorize_peer_identity(
            PeerIdentity {
                uid: 1000,
                pid: None,
            },
            |_| panic!("missing pid should fail before /proc lookup"),
        );

        assert_eq!(result, Err(AuthError::MissingPid { uid: 1000 }));
    }

    #[test]
    fn rejects_proc_exe_read_failures() {
        let result = authorize_peer_identity(
            PeerIdentity {
                uid: 1000,
                pid: Some(42),
            },
            |_| Err(std::io::Error::new(ErrorKind::NotFound, "no exe")),
        );

        assert_eq!(
            result,
            Err(AuthError::ReadExe {
                pid: 42,
                message: "no exe".to_string(),
            })
        );
    }

    #[test]
    fn rejects_deleted_executable() {
        let result = authorize_peer_identity(
            PeerIdentity {
                uid: 1000,
                pid: Some(42),
            },
            |_| Ok(PathBuf::from("/usr/bin/qsovd (deleted)")),
        );

        assert_eq!(
            result,
            Err(AuthError::DeletedExe {
                pid: 42,
                exe: "qsovd (deleted)".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn abstract_socket_smoke_has_no_filesystem_socket() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let abstract_name = format!("quicksov-abstract-smoke-{unique}");
        let abstract_addr = format!("\0{abstract_name}");
        let pseudo_path = format!("/tmp/{abstract_name}.sock");

        let listener = match UnixListener::bind(abstract_addr.as_str()) {
            Ok(listener) => listener,
            Err(error) if error.kind() == ErrorKind::PermissionDenied => return,
            Err(error) => panic!("bind abstract listener: {error}"),
        };
        let client_task = tokio::spawn({
            let abstract_addr = abstract_addr.clone();
            async move {
                UnixStream::connect(abstract_addr.as_str())
                    .await
                    .expect("connect")
            }
        });

        let (_server_stream, peer_addr) = listener.accept().await.expect("accept");
        let client_stream = client_task.await.expect("join client");

        assert_eq!(
            listener
                .local_addr()
                .expect("local addr")
                .as_abstract_name(),
            Some(abstract_name.as_bytes())
        );
        assert!(peer_addr.as_pathname().is_none());
        assert!(!Path::new(&pseudo_path).exists());

        drop(client_stream);
    }

    #[tokio::test]
    async fn permission_denied_response_round_trips_without_writing_backend() {
        let dir = TestDir::new();
        let paths = dir.paths();
        fs::write(&paths.profile_path, "balanced").expect("write profile");
        fs::write(&paths.choices_path, "low-power balanced performance").expect("write choices");

        let (client, mut server) = UnixStream::pair().expect("socket pair");
        let server_task = tokio::spawn(async move {
            write_response(
                &mut server,
                &HelperResponse::Error {
                    kind: HelperErrorKind::PermissionDenied,
                    message: "helper rejected caller".to_string(),
                },
            )
            .await
        });

        let (reader_half, writer_half) = client.into_split();
        match server_task.await.expect("join server") {
            Ok(()) => {}
            Err(super::MainError::Transport(transport::TransportError::Io(error)))
                if error.kind() == ErrorKind::PermissionDenied =>
            {
                drop(writer_half);
                return;
            }
            Err(error) => panic!("server ok: {error:?}"),
        }

        let mut reader = BufReader::new(reader_half);
        let line = transport::read_line(&mut reader)
            .await
            .expect("read response")
            .expect("response line");
        let response: HelperResponse =
            codec::decode(line.trim_end_matches('\n')).expect("decode response");

        match response {
            HelperResponse::Error { kind, message } => {
                assert_eq!(kind, HelperErrorKind::PermissionDenied);
                assert_eq!(message, "helper rejected caller");
            }
            other => panic!("unexpected response: {other:?}"),
        }
        assert_eq!(
            fs::read_to_string(&dir.paths().profile_path).expect("read profile"),
            "balanced"
        );

        drop(writer_half);
    }
}
