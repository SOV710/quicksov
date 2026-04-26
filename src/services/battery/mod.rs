// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `battery` service — sysfs battery telemetry + platform_profile integration.

pub(crate) mod helper_protocol;
pub(crate) mod power_profile;
pub(crate) mod sysfs;

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};
use tokio::time::{sleep_until, Instant};
use tracing::{debug, info, warn};
use zbus::{message::Type as MessageType, MatchRule, MessageStream};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::ipc::{codec, transport};
use crate::services::battery::helper_protocol::{HelperErrorKind, HelperRequest, HelperResponse};
use crate::services::battery::power_profile::{
    read_power_profile_state, PlatformProfilePaths, PowerProfileReason, PowerProfileState,
    ProductPowerProfile,
};
use crate::services::battery::sysfs::{read_battery_telemetry, BatteryTelemetry};
use crate::util::json_map;

const POWER_SUPPLY_ROOT: &str = "/sys/class/power_supply";
const FAST_POLL_INTERVAL: Duration = Duration::from_secs(25);
const SLOW_POLL_INTERVAL: Duration = Duration::from_secs(90);
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(150);
const HELPER_TIMEOUT: Duration = Duration::from_secs(3);
const LOGIND_DEST: &str = "org.freedesktop.login1";
const LOGIND_PATH: &str = "/org/freedesktop/login1";
const LOGIND_IFACE: &str = "org.freedesktop.login1.Manager";

pub fn spawn(_cfg: &Config) -> ServiceHandle {
    let initial = build_snapshot(&BatteryCombinedState::backend_unavailable());
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

#[derive(Debug, Clone)]
struct BatteryCombinedState {
    telemetry: BatteryTelemetry,
    power_profile: PowerProfileState,
}

impl BatteryCombinedState {
    fn backend_unavailable() -> Self {
        Self {
            telemetry: BatteryTelemetry::backend_unavailable(),
            power_profile: PowerProfileState {
                profile: ProductPowerProfile::Unknown,
                available: false,
                backend: power_profile::PowerProfileBackend::None,
                reason: Some(PowerProfileReason::Unsupported),
                choices: Vec::new(),
            },
        }
    }

    fn with_reason_override(&self, reason: PowerProfileReason) -> Self {
        Self {
            telemetry: self.telemetry.clone(),
            power_profile: self.power_profile.with_reason_override(reason),
        }
    }
}

#[derive(Debug)]
struct BatteryPaths {
    power_supply_root: PathBuf,
    profile: PlatformProfilePaths,
}

impl Default for BatteryPaths {
    fn default() -> Self {
        Self {
            power_supply_root: PathBuf::from(POWER_SUPPLY_ROOT),
            profile: PlatformProfilePaths::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RefreshHint {
    PowerSupplyChanged,
    Resume,
}

#[derive(Debug, thiserror::Error)]
enum SetPowerProfileError {
    #[error("missing 'profile' string field")]
    MissingProfile,
    #[error("invalid power profile {0:?}")]
    InvalidProfile(String),
    #[error("helper socket unavailable: {0}")]
    HelperUnavailable(String),
    #[error("permission denied while contacting helper: {0}")]
    PermissionDenied(String),
    #[error("power-profile backend unavailable: {0}")]
    BackendUnavailable(String),
    #[error("power-profile write failed: {0}")]
    WriteFailed(String),
    #[error("requested power profile is not supported on this system")]
    Unsupported,
    #[error("helper protocol error: {0}")]
    Protocol(String),
}

impl SetPowerProfileError {
    fn reason(&self) -> Option<PowerProfileReason> {
        match self {
            Self::MissingProfile | Self::InvalidProfile(_) => None,
            Self::HelperUnavailable(_) | Self::Protocol(_) => {
                Some(PowerProfileReason::HelperUnavailable)
            }
            Self::PermissionDenied(_) => Some(PowerProfileReason::PermissionDenied),
            Self::BackendUnavailable(_) => Some(PowerProfileReason::BackendUnavailable),
            Self::WriteFailed(_) => Some(PowerProfileReason::WriteFailed),
            Self::Unsupported => Some(PowerProfileReason::Unsupported),
        }
    }

    fn into_service_error(self) -> ServiceError {
        match self {
            Self::MissingProfile => ServiceError::ActionPayload {
                msg: "missing 'profile' string field".to_string(),
            },
            Self::InvalidProfile(profile) => ServiceError::ActionPayload {
                msg: format!("invalid power profile {profile:?}"),
            },
            Self::HelperUnavailable(_) | Self::BackendUnavailable(_) | Self::Unsupported => {
                ServiceError::Unavailable
            }
            Self::PermissionDenied(message) => ServiceError::Permission { msg: message },
            Self::WriteFailed(message) | Self::Protocol(message) => {
                ServiceError::Internal { msg: message }
            }
        }
    }
}

async fn run(mut request_rx: mpsc::Receiver<ServiceRequest>, state_tx: watch::Sender<Value>) {
    info!("battery service started");

    let paths = BatteryPaths::default();
    let (hint_tx, mut hint_rx) = mpsc::channel::<RefreshHint>(32);
    spawn_uevent_listener(hint_tx.clone());
    spawn_logind_resume_listener(hint_tx);

    let mut current_state = read_state(&paths);
    state_tx.send_replace(build_snapshot(&current_state));

    let far_future = Instant::now() + Duration::from_secs(365 * 24 * 60 * 60);
    let mut poll_sleep =
        std::pin::pin!(sleep_until(Instant::now() + poll_interval(&current_state)));
    let mut debounce_sleep = std::pin::pin!(sleep_until(far_future));
    let mut debounce_active = false;

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                let mut reason_override = None;
                let mut should_refresh = false;
                let reply_result = match req.action.as_str() {
                    "set_power_profile" => {
                        should_refresh = true;
                        match handle_set_power_profile(&req.payload, &paths).await {
                            Ok(()) => Ok(Value::Null),
                            Err(error) => {
                                reason_override = error.reason();
                                Err(error.into_service_error())
                            }
                        }
                    }
                    other => Err(ServiceError::ActionUnknown {
                        action: other.to_string(),
                    }),
                };
                req.reply.send(reply_result).ok();

                if should_refresh {
                    current_state = read_state(&paths);
                    let published = match reason_override {
                        Some(reason) => current_state.with_reason_override(reason),
                        None => current_state.clone(),
                    };
                    state_tx.send_replace(build_snapshot(&published));
                    poll_sleep
                        .as_mut()
                        .reset(Instant::now() + poll_interval(&current_state));
                }
            }
            hint = hint_rx.recv() => {
                let Some(_hint) = hint else { break };
                debounce_active = true;
                debounce_sleep.as_mut().reset(Instant::now() + REFRESH_DEBOUNCE);
            }
            _ = debounce_sleep.as_mut(), if debounce_active => {
                debounce_active = false;
                current_state = read_state(&paths);
                state_tx.send_replace(build_snapshot(&current_state));
                poll_sleep.as_mut().reset(Instant::now() + poll_interval(&current_state));
            }
            _ = poll_sleep.as_mut() => {
                current_state = read_state(&paths);
                state_tx.send_replace(build_snapshot(&current_state));
                poll_sleep.as_mut().reset(Instant::now() + poll_interval(&current_state));
            }
        }
    }

    info!("battery service stopped");
}

fn read_state(paths: &BatteryPaths) -> BatteryCombinedState {
    let telemetry = match read_battery_telemetry(&paths.power_supply_root) {
        Ok(telemetry) => telemetry,
        Err(error) => {
            debug!(error = %error, "battery sysfs refresh failed");
            BatteryTelemetry::backend_unavailable()
        }
    };
    let power_profile = read_power_profile_state(&paths.profile);

    BatteryCombinedState {
        telemetry,
        power_profile,
    }
}

fn poll_interval(state: &BatteryCombinedState) -> Duration {
    match state.telemetry.state {
        sysfs::ChargeState::Charging | sysfs::ChargeState::Discharging => FAST_POLL_INTERVAL,
        _ => SLOW_POLL_INTERVAL,
    }
}

async fn handle_set_power_profile(
    payload: &Value,
    paths: &BatteryPaths,
) -> Result<(), SetPowerProfileError> {
    let profile = extract_str(payload, "profile").ok_or(SetPowerProfileError::MissingProfile)?;
    let requested = ProductPowerProfile::from_action_str(profile)
        .ok_or_else(|| SetPowerProfileError::InvalidProfile(profile.to_string()))?;

    let stream = tokio::time::timeout(
        HELPER_TIMEOUT,
        UnixStream::connect(&paths.profile.helper_socket_path),
    )
    .await
    .map_err(|_| SetPowerProfileError::HelperUnavailable("helper connect timed out".to_string()))?
    .map_err(map_helper_connect_error)?;

    let request = HelperRequest {
        action: HelperRequest::SET_PLATFORM_PROFILE_ACTION.to_string(),
        profile: requested.as_str().to_string(),
    };
    let encoded = codec::encode(&request)
        .map_err(|error| SetPowerProfileError::Protocol(error.to_string()))?;

    let (reader, mut writer) = stream.into_split();
    transport::write_line(&mut writer, &encoded)
        .await
        .map_err(|error| SetPowerProfileError::HelperUnavailable(error.to_string()))?;

    let mut reader = BufReader::new(reader);
    let line = transport::read_line(&mut reader)
        .await
        .map_err(|error| SetPowerProfileError::HelperUnavailable(error.to_string()))?
        .ok_or_else(|| {
            SetPowerProfileError::HelperUnavailable("helper closed connection".to_string())
        })?;
    let response: HelperResponse = codec::decode(line.trim_end_matches('\n'))
        .map_err(|error| SetPowerProfileError::Protocol(error.to_string()))?;

    match response {
        HelperResponse::Ok { profile, .. } => {
            if profile == requested.as_str() {
                Ok(())
            } else {
                Err(SetPowerProfileError::WriteFailed(format!(
                    "helper applied unexpected profile {profile:?}"
                )))
            }
        }
        HelperResponse::Error { kind, message } => match kind {
            HelperErrorKind::Unsupported => Err(SetPowerProfileError::Unsupported),
            HelperErrorKind::PermissionDenied => {
                Err(SetPowerProfileError::PermissionDenied(message))
            }
            HelperErrorKind::BackendUnavailable => {
                Err(SetPowerProfileError::BackendUnavailable(message))
            }
            HelperErrorKind::WriteFailed => Err(SetPowerProfileError::WriteFailed(message)),
            HelperErrorKind::InvalidRequest => Err(SetPowerProfileError::Protocol(message)),
        },
    }
}

fn map_helper_connect_error(error: std::io::Error) -> SetPowerProfileError {
    match error.kind() {
        std::io::ErrorKind::NotFound
        | std::io::ErrorKind::ConnectionRefused
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::TimedOut => {
            SetPowerProfileError::HelperUnavailable(error.to_string())
        }
        std::io::ErrorKind::PermissionDenied => {
            SetPowerProfileError::PermissionDenied(error.to_string())
        }
        _ => SetPowerProfileError::HelperUnavailable(error.to_string()),
    }
}

fn extract_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.as_object()?.get(key)?.as_str()
}

fn build_snapshot(state: &BatteryCombinedState) -> Value {
    let telemetry = &state.telemetry;
    let power_profile = &state.power_profile;

    json_map([
        ("availability", Value::from(telemetry.availability.as_str())),
        ("present", Value::Bool(telemetry.present)),
        ("on_battery", Value::Bool(telemetry.on_battery)),
        ("level", Value::from(telemetry.level)),
        ("state", Value::from(telemetry.state.as_str())),
        ("time_to_empty_sec", Value::Null),
        ("time_to_full_sec", Value::Null),
        ("health_percent", float_value(telemetry.health_percent)),
        ("energy_rate_w", float_value(telemetry.energy_rate_w)),
        ("energy_now_wh", float_value(telemetry.energy_now_wh)),
        ("energy_full_wh", float_value(telemetry.energy_full_wh)),
        ("energy_design_wh", float_value(telemetry.energy_design_wh)),
        (
            "batteries",
            Value::Array(
                telemetry
                    .batteries
                    .iter()
                    .map(|battery| {
                        json_map([
                            ("name", Value::from(battery.name.as_str())),
                            ("present", Value::Bool(battery.present)),
                            ("level", Value::from(battery.level)),
                            ("state", Value::from(battery.state.as_str())),
                            ("health_percent", float_value(battery.health_percent)),
                            ("energy_rate_w", float_value(battery.energy_rate_w)),
                            ("energy_now_wh", float_value(battery.energy_now_wh)),
                            ("energy_full_wh", float_value(battery.energy_full_wh)),
                            ("energy_design_wh", float_value(battery.energy_design_wh)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("power_profile", Value::from(power_profile.profile.as_str())),
        (
            "power_profile_available",
            Value::Bool(power_profile.available),
        ),
        (
            "power_profile_backend",
            Value::from(power_profile.backend.as_str()),
        ),
        (
            "power_profile_reason",
            power_profile
                .reason
                .map(|reason| Value::from(reason.as_str()))
                .unwrap_or(Value::Null),
        ),
        (
            "power_profile_choices",
            Value::Array(
                power_profile
                    .choices
                    .iter()
                    .map(|choice| Value::from(choice.as_str()))
                    .collect(),
            ),
        ),
    ])
}

fn float_value(value: Option<f64>) -> Value {
    value
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn spawn_uevent_listener(hint_tx: mpsc::Sender<RefreshHint>) {
    std::thread::spawn(move || {
        let socket = match bind_uevent_socket() {
            Ok(socket) => socket,
            Err(error) => {
                warn!(error = %error, "battery uevent listener unavailable; polling only");
                return;
            }
        };

        let mut buffer = [0_u8; 8192];
        loop {
            match recv_is_power_supply_event(&socket, &mut buffer) {
                Ok(true) => {
                    if hint_tx
                        .blocking_send(RefreshHint::PowerSupplyChanged)
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    warn!(error = %error, "battery uevent listener stopped; polling only");
                    break;
                }
            }
        }
    });
}

fn bind_uevent_socket() -> Result<OwnedFd, std::io::Error> {
    // SAFETY: `socket` returns a fresh file descriptor on success. We either wrap it
    // in `OwnedFd` or close it on failure paths before returning.
    let fd = unsafe {
        nix::libc::socket(
            nix::libc::AF_NETLINK,
            nix::libc::SOCK_RAW | nix::libc::SOCK_CLOEXEC,
            nix::libc::NETLINK_KOBJECT_UEVENT,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut addr: nix::libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    addr.nl_family = nix::libc::AF_NETLINK as u16;
    addr.nl_pid = 0;
    addr.nl_groups = 1;
    let bind_result = unsafe {
        nix::libc::bind(
            fd,
            &addr as *const _ as *const nix::libc::sockaddr,
            std::mem::size_of::<nix::libc::sockaddr_nl>() as nix::libc::socklen_t,
        )
    };
    if bind_result < 0 {
        let error = std::io::Error::last_os_error();
        let _ = unsafe { nix::libc::close(fd) };
        return Err(error);
    }

    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn recv_is_power_supply_event(socket: &OwnedFd, buffer: &mut [u8]) -> Result<bool, std::io::Error> {
    // SAFETY: `buffer` is valid for writes, and `socket` is an open netlink fd.
    let received = unsafe {
        nix::libc::recv(
            socket.as_raw_fd(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            0,
        )
    };
    if received < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(parse_power_supply_uevent(&buffer[..received as usize]))
}

fn parse_power_supply_uevent(payload: &[u8]) -> bool {
    payload
        .split(|byte| *byte == 0)
        .filter_map(|field| std::str::from_utf8(field).ok())
        .any(|field| field == "SUBSYSTEM=power_supply")
}

fn spawn_logind_resume_listener(hint_tx: mpsc::Sender<RefreshHint>) {
    tokio::spawn(async move {
        loop {
            match watch_logind_prepare_for_sleep(&hint_tx).await {
                Ok(()) => break,
                Err(error) => {
                    warn!(error = %error, "battery resume listener unavailable; retrying in 5 s");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });
}

async fn watch_logind_prepare_for_sleep(hint_tx: &mpsc::Sender<RefreshHint>) -> Result<(), String> {
    let conn = zbus::Connection::system()
        .await
        .map_err(|error| error.to_string())?;
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(LOGIND_DEST)
        .map_err(|error| error.to_string())?
        .interface(LOGIND_IFACE)
        .map_err(|error| error.to_string())?
        .member("PrepareForSleep")
        .map_err(|error| error.to_string())?
        .path(LOGIND_PATH)
        .map_err(|error| error.to_string())?
        .build();
    let mut stream = MessageStream::for_match_rule(rule, &conn, None)
        .await
        .map_err(|error| error.to_string())?;

    while let Some(message) = stream.next().await {
        let message = message.map_err(|error| error.to_string())?;
        let (preparing,): (bool,) = message
            .body()
            .deserialize()
            .map_err(|error| error.to_string())?;
        if !preparing && hint_tx.send(RefreshHint::Resume).await.is_err() {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::bus::ServiceError;

    use super::{handle_set_power_profile, BatteryPaths, SetPowerProfileError};

    #[tokio::test]
    async fn invalid_payload_is_rejected_before_helper_contact() {
        let paths = BatteryPaths::default();
        let error = handle_set_power_profile(&json!({}), &paths)
            .await
            .expect_err("payload should fail");
        assert!(matches!(error, SetPowerProfileError::MissingProfile));
    }

    #[tokio::test]
    async fn invalid_profile_value_is_rejected_before_helper_contact() {
        let paths = BatteryPaths::default();
        let error = handle_set_power_profile(&json!({ "profile": "turbo" }), &paths)
            .await
            .expect_err("profile should fail");
        match error.into_service_error() {
            ServiceError::ActionPayload { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
