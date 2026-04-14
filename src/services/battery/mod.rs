// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `battery` service — UPower + PowerProfiles via D-Bus.

use rmpv::Value;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::rmpv_map;

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
    rmpv_map([
        ("present", Value::Boolean(false)),
        ("on_battery", Value::Boolean(false)),
        ("level", Value::from(0)),
        ("state", Value::from("unknown")),
        ("time_to_empty_sec", Value::Nil),
        ("time_to_full_sec", Value::Nil),
        ("power_profile", Value::from("unknown")),
    ])
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
    let upower = build_proxy(
        conn,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower",
        "org.freedesktop.UPower",
    )
    .await?;

    let device = build_proxy(
        conn,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower/devices/DisplayDevice",
        "org.freedesktop.DBus.Properties",
    )
    .await?;

    let pp_proxy = build_power_profiles_proxy(conn).await;

    // Initial snapshot
    let snap = read_full_snapshot(&upower, &device, &pp_proxy).await;
    state_tx.send_replace(snap);

    // Subscribe to property changes on the UPower device
    let mut device_changes = device
        .receive_signal("PropertiesChanged")
        .await
        .map_err(|e| BatteryError::Dbus(e.to_string()))?;

    use futures::StreamExt;
    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                handle_request(req, conn, &pp_proxy).await;
            }
            msg = device_changes.next() => {
                if msg.is_none() { break; }
                let snap = read_full_snapshot(&upower, &device, &pp_proxy).await;
                state_tx.send_replace(snap);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Snapshot reading
// ---------------------------------------------------------------------------

async fn read_full_snapshot(
    upower: &zbus::Proxy<'_>,
    device: &zbus::Proxy<'_>,
    pp_proxy: &Option<zbus::Proxy<'_>>,
) -> Value {
    let on_battery = get_prop::<bool>(upower, "OnBattery").await.unwrap_or(false);
    let present = get_dev_prop::<bool>(device, "IsPresent")
        .await
        .unwrap_or(false);
    let level = get_dev_prop::<f64>(device, "Percentage")
        .await
        .unwrap_or(0.0) as i64;
    let state_u32 = get_dev_prop::<u32>(device, "State").await.unwrap_or(0);
    let tte = get_dev_prop::<i64>(device, "TimeToEmpty").await.ok();
    let ttf = get_dev_prop::<i64>(device, "TimeToFull").await.ok();

    let profile = match pp_proxy {
        Some(pp) => get_prop::<String>(pp, "ActiveProfile")
            .await
            .unwrap_or_else(|_| "unknown".to_string()),
        None => "unknown".to_string(),
    };

    build_snapshot(on_battery, present, level, state_u32, tte, ttf, &profile)
}

fn build_snapshot(
    on_battery: bool,
    present: bool,
    level: i64,
    state_u32: u32,
    tte: Option<i64>,
    ttf: Option<i64>,
    profile: &str,
) -> Value {
    let tte_val = match tte {
        Some(v) if v > 0 => Value::from(v),
        _ => Value::Nil,
    };
    let ttf_val = match ttf {
        Some(v) if v > 0 => Value::from(v),
        _ => Value::Nil,
    };
    rmpv_map([
        ("present", Value::Boolean(present)),
        ("on_battery", Value::Boolean(on_battery)),
        ("level", Value::from(level)),
        ("state", Value::from(upower_state_str(state_u32))),
        ("time_to_empty_sec", tte_val),
        ("time_to_full_sec", ttf_val),
        ("power_profile", Value::from(profile)),
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
// Request handling
// ---------------------------------------------------------------------------

async fn handle_request(
    req: ServiceRequest,
    conn: &zbus::Connection,
    pp_proxy: &Option<zbus::Proxy<'_>>,
) {
    let result = match req.action.as_str() {
        "set_power_profile" => handle_set_power_profile(&req.payload, conn, pp_proxy).await,
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    req.reply.send(result).ok();
}

async fn handle_set_power_profile(
    payload: &Value,
    conn: &zbus::Connection,
    pp_proxy: &Option<zbus::Proxy<'_>>,
) -> Result<Value, ServiceError> {
    let profile = extract_str(payload, "profile").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'profile' string field".to_string(),
    })?;

    let _pp = pp_proxy.as_ref().ok_or_else(|| ServiceError::Internal {
        msg: "power profiles D-Bus not available".to_string(),
    })?;

    debug!(profile = %profile, "setting power profile");

    let props_proxy = build_proxy(
        conn,
        "net.hadess.PowerProfiles",
        "/net/hadess/PowerProfiles",
        "org.freedesktop.DBus.Properties",
    )
    .await
    .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;

    let variant = zbus::zvariant::Value::from(profile.to_string());
    let _: () = props_proxy
        .call(
            "Set",
            &("net.hadess.PowerProfiles", "ActiveProfile", variant),
        )
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;

    Ok(Value::Nil)
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
        "net.hadess.PowerProfiles",
        "/net/hadess/PowerProfiles",
        "net.hadess.PowerProfiles",
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
        .call("Get", &("org.freedesktop.UPower.Device", name))
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
    if let Value::Map(pairs) = v {
        for (k, val) in pairs {
            if k.as_str() == Some(key) {
                return val.as_str();
            }
        }
    }
    None
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
