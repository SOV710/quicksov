// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `battery` service — UPower + PowerProfiles via D-Bus.

use std::collections::HashMap;

use futures::StreamExt;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};
use zbus::zvariant::OwnedValue;
use zbus::{message::Type as MessageType, MatchRule, MessageStream};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::json_map;

const UPOWER_DEST: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UPOWER_IFACE: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DISPLAY_DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";
const POWER_PROFILES_DEST: &str = "net.hadess.PowerProfiles";
const POWER_PROFILES_PATH: &str = "/net/hadess/PowerProfiles";
const POWER_PROFILES_IFACE: &str = "net.hadess.PowerProfiles";
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";

type PropertiesChangedBody = (String, HashMap<String, OwnedValue>, Vec<String>);

/// Spawn the `battery` service task and return its [`ServiceHandle`].
pub fn spawn(_cfg: &Config) -> ServiceHandle {
    let initial = unavailable_snapshot();
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

fn unavailable_snapshot() -> Value {
    json_map([
        ("present", Value::Bool(false)),
        ("on_battery", Value::Bool(false)),
        ("level", Value::from(0)),
        ("state", Value::from("unknown")),
        ("time_to_empty_sec", Value::Null),
        ("time_to_full_sec", Value::Null),
        ("power_profile", Value::from("unknown")),
    ])
}

#[derive(Clone)]
struct BatteryState {
    present: bool,
    on_battery: bool,
    level: i64,
    state_u32: u32,
    time_to_empty_sec: Option<i64>,
    time_to_full_sec: Option<i64>,
    power_profile: String,
}

impl BatteryState {
    fn unavailable() -> Self {
        Self {
            present: false,
            on_battery: false,
            level: 0,
            state_u32: 0,
            time_to_empty_sec: None,
            time_to_full_sec: None,
            power_profile: "unknown".to_string(),
        }
    }
}

async fn run(mut request_rx: mpsc::Receiver<ServiceRequest>, state_tx: watch::Sender<Value>) {
    info!("battery service started");
    loop {
        match connect_and_run(&mut request_rx, &state_tx).await {
            Ok(()) => break,
            Err(e) => {
                warn!(error = %e, "battery D-Bus connection failed; retrying in 5 s");
                state_tx.send_replace(unavailable_snapshot());
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
    info!("battery service stopped");
}

// ---------------------------------------------------------------------------
// D-Bus connection
// ---------------------------------------------------------------------------

async fn connect_and_run(
    request_rx: &mut mpsc::Receiver<ServiceRequest>,
    state_tx: &watch::Sender<Value>,
) -> Result<(), BatteryError> {
    let conn = zbus::Connection::system().await?;
    run_connected(request_rx, state_tx, &conn).await
}

async fn run_connected(
    request_rx: &mut mpsc::Receiver<ServiceRequest>,
    state_tx: &watch::Sender<Value>,
    conn: &zbus::Connection,
) -> Result<(), BatteryError> {
    let upower = build_proxy(conn, UPOWER_DEST, UPOWER_PATH, UPOWER_IFACE).await?;
    let device = build_proxy(conn, UPOWER_DEST, DISPLAY_DEVICE_PATH, PROPERTIES_IFACE).await?;
    let mut pp_proxy = build_power_profiles_proxy(conn).await;

    let mut state = read_full_state(&upower, &device, conn, &mut pp_proxy).await;
    state_tx.send_replace(build_snapshot(&state));

    let mut upower_changes = property_stream(conn, UPOWER_DEST, UPOWER_PATH, UPOWER_IFACE).await?;
    let mut device_changes =
        property_stream(conn, UPOWER_DEST, DISPLAY_DEVICE_PATH, DISPLAY_DEVICE_IFACE).await?;
    let mut power_profile_changes = property_stream(
        conn,
        POWER_PROFILES_DEST,
        POWER_PROFILES_PATH,
        POWER_PROFILES_IFACE,
    )
    .await?;

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                handle_request(req, conn).await;
            }
            msg = upower_changes.next() => {
                let Some(msg) = msg else { break };
                let msg = msg?;
                apply_upower_change(&mut state, &msg, &upower).await?;
                state_tx.send_replace(build_snapshot(&state));
            }
            msg = device_changes.next() => {
                let Some(msg) = msg else { break };
                let msg = msg?;
                apply_device_change(&mut state, &msg, &device).await?;
                state_tx.send_replace(build_snapshot(&state));
            }
            msg = power_profile_changes.next() => {
                let Some(msg) = msg else { break };
                let msg = msg?;
                apply_power_profile_change(&mut state, &msg, conn, &mut pp_proxy).await?;
                state_tx.send_replace(build_snapshot(&state));
            }
        }
    }
    Ok(())
}

async fn property_stream(
    conn: &zbus::Connection,
    sender: &'static str,
    path: &'static str,
    iface_arg0: &'static str,
) -> Result<MessageStream, BatteryError> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(sender)?
        .interface(PROPERTIES_IFACE)?
        .member("PropertiesChanged")?
        .path(path)?
        .add_arg(iface_arg0)?
        .build();
    MessageStream::for_match_rule(rule, conn, None)
        .await
        .map_err(BatteryError::Zbus)
}

// ---------------------------------------------------------------------------
// Snapshot reading
// ---------------------------------------------------------------------------

async fn read_full_state<'a>(
    upower: &zbus::Proxy<'_>,
    device: &zbus::Proxy<'_>,
    conn: &'a zbus::Connection,
    pp_proxy: &mut Option<zbus::Proxy<'a>>,
) -> BatteryState {
    let mut state = BatteryState::unavailable();

    state.on_battery = get_prop::<bool>(upower, "OnBattery").await.unwrap_or(false);
    state.present = get_dev_prop::<bool>(device, "IsPresent")
        .await
        .unwrap_or(false);
    state.level = get_dev_prop::<f64>(device, "Percentage")
        .await
        .unwrap_or(0.0) as i64;
    state.state_u32 = get_dev_prop::<u32>(device, "State").await.unwrap_or(0);
    state.time_to_empty_sec = get_dev_prop::<i64>(device, "TimeToEmpty").await.ok();
    state.time_to_full_sec = get_dev_prop::<i64>(device, "TimeToFull").await.ok();
    state.power_profile = read_power_profile(conn, pp_proxy).await;

    state
}

fn build_snapshot(state: &BatteryState) -> Value {
    let tte_val = match state.time_to_empty_sec {
        Some(v) if v > 0 => Value::from(v),
        _ => Value::Null,
    };
    let ttf_val = match state.time_to_full_sec {
        Some(v) if v > 0 => Value::from(v),
        _ => Value::Null,
    };
    json_map([
        ("present", Value::Bool(state.present)),
        ("on_battery", Value::Bool(state.on_battery)),
        ("level", Value::from(state.level)),
        ("state", Value::from(upower_state_str(state.state_u32))),
        ("time_to_empty_sec", tte_val),
        ("time_to_full_sec", ttf_val),
        ("power_profile", Value::from(state.power_profile.as_str())),
    ])
}

fn upower_state_str(state: u32) -> &'static str {
    match state {
        1 => "charging",
        2 => "discharging",
        3 => "empty",
        4 => "fully_charged",
        5 => "pending_charge",
        6 => "pending_discharge",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Subscription handlers
// ---------------------------------------------------------------------------

async fn apply_upower_change(
    state: &mut BatteryState,
    msg: &zbus::Message,
    upower: &zbus::Proxy<'_>,
) -> Result<(), BatteryError> {
    let (_, changed, invalidated) = properties_changed_body(msg)?;
    if let Some(on_battery) = owned_prop::<bool>(&changed, "OnBattery") {
        state.on_battery = on_battery;
    }
    if invalidated.iter().any(|name| name == "OnBattery") {
        state.on_battery = get_prop::<bool>(upower, "OnBattery").await.unwrap_or(false);
    }
    Ok(())
}

async fn apply_device_change(
    state: &mut BatteryState,
    msg: &zbus::Message,
    device: &zbus::Proxy<'_>,
) -> Result<(), BatteryError> {
    let (_, changed, invalidated) = properties_changed_body(msg)?;

    if let Some(present) = owned_prop::<bool>(&changed, "IsPresent") {
        state.present = present;
    }
    if let Some(level) = owned_prop::<f64>(&changed, "Percentage") {
        state.level = level as i64;
    }
    if let Some(state_u32) = owned_prop::<u32>(&changed, "State") {
        state.state_u32 = state_u32;
    }
    if let Some(time) = owned_prop::<i64>(&changed, "TimeToEmpty") {
        state.time_to_empty_sec = positive_time(time);
    }
    if let Some(time) = owned_prop::<i64>(&changed, "TimeToFull") {
        state.time_to_full_sec = positive_time(time);
    }

    refresh_invalidated_device_props(state, device, &invalidated).await;
    Ok(())
}

async fn apply_power_profile_change<'a>(
    state: &mut BatteryState,
    msg: &zbus::Message,
    conn: &'a zbus::Connection,
    pp_proxy: &mut Option<zbus::Proxy<'a>>,
) -> Result<(), BatteryError> {
    let (_, changed, invalidated) = properties_changed_body(msg)?;

    if let Some(profile) = owned_prop::<String>(&changed, "ActiveProfile") {
        state.power_profile = profile;
    }
    if invalidated.iter().any(|name| name == "ActiveProfile") {
        state.power_profile = read_power_profile(conn, pp_proxy).await;
    }
    Ok(())
}

fn properties_changed_body(msg: &zbus::Message) -> Result<PropertiesChangedBody, BatteryError> {
    msg.body()
        .deserialize()
        .map_err(|e| BatteryError::Dbus(e.to_string()))
}

async fn refresh_invalidated_device_props(
    state: &mut BatteryState,
    device: &zbus::Proxy<'_>,
    invalidated: &[String],
) {
    for key in invalidated {
        match key.as_str() {
            "IsPresent" => {
                state.present = get_dev_prop::<bool>(device, "IsPresent")
                    .await
                    .unwrap_or(false);
            }
            "Percentage" => {
                state.level = get_dev_prop::<f64>(device, "Percentage")
                    .await
                    .unwrap_or(0.0) as i64;
            }
            "State" => {
                state.state_u32 = get_dev_prop::<u32>(device, "State").await.unwrap_or(0);
            }
            "TimeToEmpty" => {
                state.time_to_empty_sec = get_dev_prop::<i64>(device, "TimeToEmpty")
                    .await
                    .ok()
                    .and_then(positive_time);
            }
            "TimeToFull" => {
                state.time_to_full_sec = get_dev_prop::<i64>(device, "TimeToFull")
                    .await
                    .ok()
                    .and_then(positive_time);
            }
            _ => {}
        }
    }
}

fn positive_time(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

fn owned_prop<T>(props: &HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    props
        .get(key)
        .and_then(|value| T::try_from(value.clone()).ok())
}

async fn read_power_profile<'a>(
    conn: &'a zbus::Connection,
    pp_proxy: &mut Option<zbus::Proxy<'a>>,
) -> String {
    if pp_proxy.is_none() {
        *pp_proxy = build_power_profiles_proxy(conn).await;
    }

    match pp_proxy.as_ref() {
        Some(proxy) => get_prop::<String>(proxy, "ActiveProfile")
            .await
            .unwrap_or_else(|_| "unknown".to_string()),
        None => "unknown".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------

async fn handle_request(req: ServiceRequest, conn: &zbus::Connection) {
    let result = match req.action.as_str() {
        "set_power_profile" => handle_set_power_profile(&req.payload, conn).await,
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    req.reply.send(result).ok();
}

async fn handle_set_power_profile(
    payload: &Value,
    conn: &zbus::Connection,
) -> Result<Value, ServiceError> {
    let profile = extract_str(payload, "profile").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'profile' string field".to_string(),
    })?;

    debug!(profile = %profile, "setting power profile");

    let props_proxy = build_proxy(
        conn,
        POWER_PROFILES_DEST,
        POWER_PROFILES_PATH,
        PROPERTIES_IFACE,
    )
    .await
    .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;

    let variant = zbus::zvariant::Value::from(profile.to_string());
    let _: () = props_proxy
        .call("Set", &(POWER_PROFILES_IFACE, "ActiveProfile", variant))
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;

    Ok(Value::Null)
}

// ---------------------------------------------------------------------------
// D-Bus proxy helpers
// ---------------------------------------------------------------------------

async fn build_proxy<'a>(
    conn: &'a zbus::Connection,
    dest: &'static str,
    path: &'static str,
    iface: &'static str,
) -> Result<zbus::Proxy<'a>, BatteryError> {
    zbus::Proxy::new(conn, dest, path, iface)
        .await
        .map_err(BatteryError::Zbus)
}

async fn build_power_profiles_proxy(conn: &zbus::Connection) -> Option<zbus::Proxy<'_>> {
    build_proxy(
        conn,
        POWER_PROFILES_DEST,
        POWER_PROFILES_PATH,
        POWER_PROFILES_IFACE,
    )
    .await
    .ok()
}

async fn get_prop<T>(proxy: &zbus::Proxy<'_>, name: &str) -> Result<T, BatteryError>
where
    T: TryFrom<zbus::zvariant::OwnedValue>,
    T::Error: Into<zbus::Error>,
{
    proxy
        .get_property(name)
        .await
        .map_err(|e| BatteryError::Dbus(e.to_string()))
}

async fn get_dev_prop<T>(device: &zbus::Proxy<'_>, name: &str) -> Result<T, BatteryError>
where
    T: TryFrom<zbus::zvariant::OwnedValue>,
    T::Error: Into<zbus::Error>,
{
    let val: zbus::zvariant::OwnedValue = device
        .call("Get", &(DISPLAY_DEVICE_IFACE, name))
        .await
        .map_err(|e| BatteryError::Dbus(e.to_string()))?;
    let v: T = val
        .try_into()
        .map_err(|e: T::Error| BatteryError::Dbus(Into::<zbus::Error>::into(e).to_string()))?;
    Ok(v)
}

// ---------------------------------------------------------------------------
// Payload helpers
// ---------------------------------------------------------------------------

fn extract_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.as_object()?.get(key)?.as_str()
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum BatteryError {
    #[error("zbus error: {0}")]
    Zbus(#[from] zbus::Error),
    #[error("D-Bus error: {0}")]
    Dbus(String),
}
