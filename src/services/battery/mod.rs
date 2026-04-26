// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `battery` service — UPower telemetry + power-profiles-daemon integration.

pub(crate) mod power_profile;
pub(crate) mod upower;

use futures::StreamExt;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};
use zbus::{message::Type as MessageType, MatchRule, MessageStream};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::services::battery::power_profile::{
    read_power_profile_state, set_power_profile, PowerProfileReason, PowerProfileState,
    ProductPowerProfile, PPD_DEST, PPD_PATH,
};
use crate::services::battery::upower::{
    read_battery_telemetry, BatteryTelemetry, UPOWER_DEST, UPOWER_IFACE, UPOWER_PATH,
};
use crate::util::json_map;

const DBUS_DEST: &str = "org.freedesktop.DBus";
const DBUS_IFACE: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const RECONNECT_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

pub fn spawn(_cfg: &Config) -> ServiceHandle {
    let initial = build_snapshot(&BatteryCombinedState::disconnected());
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
    fn disconnected() -> Self {
        Self {
            telemetry: BatteryTelemetry::backend_unavailable(),
            power_profile: PowerProfileState::service_unavailable(),
        }
    }

    fn with_reason_override(&self, reason: PowerProfileReason) -> Self {
        Self {
            telemetry: self.telemetry.clone(),
            power_profile: self.power_profile.with_reason_override(reason),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum HandleSetPowerProfileError {
    #[error("missing 'profile' string field")]
    MissingProfile,
    #[error("invalid power profile {0:?}")]
    InvalidProfile(String),
    #[error(transparent)]
    Backend(#[from] power_profile::SetPowerProfileError),
}

impl HandleSetPowerProfileError {
    fn reason(&self) -> Option<PowerProfileReason> {
        match self {
            Self::MissingProfile | Self::InvalidProfile(_) => None,
            Self::Backend(error) => error.reason(),
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
            Self::Backend(error) => error.into_service_error(),
        }
    }
}

async fn run(mut request_rx: mpsc::Receiver<ServiceRequest>, state_tx: watch::Sender<Value>) {
    info!("battery service started");
    loop {
        match connect_and_run(&mut request_rx, &state_tx).await {
            Ok(()) => break,
            Err(error) => {
                warn!(error = %error, "battery D-Bus connection failed; retrying in 5 s");
                state_tx.send_replace(build_snapshot(&BatteryCombinedState::disconnected()));
                tokio::time::sleep(RECONNECT_DELAY).await;
            }
        }
    }
    info!("battery service stopped");
}

async fn connect_and_run(
    request_rx: &mut mpsc::Receiver<ServiceRequest>,
    state_tx: &watch::Sender<Value>,
) -> Result<(), BatteryError> {
    let conn = zbus::Connection::system().await?;
    let mut current_state = read_state(&conn).await;
    state_tx.send_replace(build_snapshot(&current_state));

    let mut upower_added = signal_stream(
        &conn,
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(UPOWER_DEST)?
            .interface(UPOWER_IFACE)?
            .member("DeviceAdded")?
            .path(UPOWER_PATH)?
            .build(),
    )
    .await?;
    let mut upower_removed = signal_stream(
        &conn,
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(UPOWER_DEST)?
            .interface(UPOWER_IFACE)?
            .member("DeviceRemoved")?
            .path(UPOWER_PATH)?
            .build(),
    )
    .await?;
    let mut upower_changed = signal_stream(
        &conn,
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(UPOWER_DEST)?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .path_namespace(UPOWER_PATH)?
            .build(),
    )
    .await?;
    let mut ppd_changed = signal_stream(
        &conn,
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(PPD_DEST)?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .path(PPD_PATH)?
            .build(),
    )
    .await?;
    let mut name_changed = signal_stream(
        &conn,
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(DBUS_DEST)?
            .interface(DBUS_IFACE)?
            .member("NameOwnerChanged")?
            .path(DBUS_PATH)?
            .build(),
    )
    .await?;

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                let mut reason_override = None;
                let mut should_refresh = false;
                let reply_result = match req.action.as_str() {
                    "set_power_profile" => {
                        should_refresh = true;
                        match handle_set_power_profile(&conn, &req.payload).await {
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
                    current_state = read_state(&conn).await;
                    let published = match reason_override {
                        Some(reason) => current_state.with_reason_override(reason),
                        None => current_state.clone(),
                    };
                    state_tx.send_replace(build_snapshot(&published));
                }
            }
            signal = upower_added.next() => {
                next_signal(signal, "UPower DeviceAdded")?;
                current_state = read_state(&conn).await;
                state_tx.send_replace(build_snapshot(&current_state));
            }
            signal = upower_removed.next() => {
                next_signal(signal, "UPower DeviceRemoved")?;
                current_state = read_state(&conn).await;
                state_tx.send_replace(build_snapshot(&current_state));
            }
            signal = upower_changed.next() => {
                next_signal(signal, "UPower PropertiesChanged")?;
                current_state = read_state(&conn).await;
                state_tx.send_replace(build_snapshot(&current_state));
            }
            signal = ppd_changed.next() => {
                next_signal(signal, "power-profiles-daemon PropertiesChanged")?;
                current_state = read_state(&conn).await;
                state_tx.send_replace(build_snapshot(&current_state));
            }
            signal = name_changed.next() => {
                let message = next_signal(signal, "D-Bus NameOwnerChanged")?;
                let (name, _old_owner, _new_owner): (String, String, String) = message
                    .body()
                    .deserialize()
                    .map_err(|error| BatteryError::Dbus(error.to_string()))?;
                if matches!(name.as_str(), UPOWER_DEST | PPD_DEST) {
                    current_state = read_state(&conn).await;
                    state_tx.send_replace(build_snapshot(&current_state));
                }
            }
        }
    }

    Ok(())
}

async fn signal_stream(
    conn: &zbus::Connection,
    rule: MatchRule<'static>,
) -> Result<MessageStream, BatteryError> {
    MessageStream::for_match_rule(rule, conn, None)
        .await
        .map_err(BatteryError::Zbus)
}

fn next_signal(
    signal: Option<Result<zbus::Message, zbus::Error>>,
    label: &str,
) -> Result<zbus::Message, BatteryError> {
    let Some(signal) = signal else {
        return Err(BatteryError::Dbus(format!("{label} stream closed")));
    };
    signal.map_err(BatteryError::Zbus)
}

async fn read_state(conn: &zbus::Connection) -> BatteryCombinedState {
    let telemetry = match read_battery_telemetry(conn).await {
        Ok(telemetry) => telemetry,
        Err(error) => {
            debug!(error = %error, "UPower refresh failed");
            BatteryTelemetry::backend_unavailable()
        }
    };
    let power_profile = read_power_profile_state(conn).await;

    BatteryCombinedState {
        telemetry,
        power_profile,
    }
}

async fn handle_set_power_profile(
    conn: &zbus::Connection,
    payload: &Value,
) -> Result<(), HandleSetPowerProfileError> {
    let profile = extract_str(payload, "profile").ok_or(HandleSetPowerProfileError::MissingProfile)?;
    let requested = ProductPowerProfile::from_action_str(profile)
        .ok_or_else(|| HandleSetPowerProfileError::InvalidProfile(profile.to_string()))?;
    set_power_profile(conn, requested).await?;
    Ok(())
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
        (
            "time_to_empty_sec",
            int_value(telemetry.time_to_empty_sec),
        ),
        (
            "time_to_full_sec",
            int_value(telemetry.time_to_full_sec),
        ),
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
        (
            "power_profile_degraded_reason",
            power_profile
                .degraded_reason
                .as_ref()
                .map(|reason| Value::from(reason.as_str()))
                .unwrap_or(Value::Null),
        ),
    ])
}

fn float_value(value: Option<f64>) -> Value {
    value
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn int_value(value: Option<i64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

#[derive(Debug, thiserror::Error)]
enum BatteryError {
    #[error("zbus error: {0}")]
    Zbus(#[from] zbus::Error),
    #[error("D-Bus error: {0}")]
    Dbus(String),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::bus::ServiceError;

    use super::HandleSetPowerProfileError;

    #[test]
    fn invalid_profile_value_maps_to_payload_error() {
        let error = HandleSetPowerProfileError::InvalidProfile("turbo".to_string());
        match error.into_service_error() {
            ServiceError::ActionPayload { msg } => assert!(msg.contains("turbo")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn extract_profile_requires_string_field() {
        let payload = json!({});
        let error = super::extract_str(&payload, "profile");
        assert_eq!(error, None);
    }
}
