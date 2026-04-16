// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `bluetooth` service — BlueZ D-Bus backend.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};
use zbus::zvariant::OwnedValue;

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::json_map;

use futures::StreamExt;

/// Spawn the `bluetooth` service and return its [`ServiceHandle`].
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
        ("powered", Value::Bool(false)),
        ("discovering", Value::Bool(false)),
        ("devices", Value::Array(vec![])),
    ])
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

struct BtState {
    powered: bool,
    discovering: bool,
    adapter_path: Option<String>,
    devices: HashMap<String, BtDevice>,
}

struct BtDevice {
    address: String,
    name: String,
    icon: String,
    paired: bool,
    connected: bool,
    trusted: bool,
    battery: Option<i64>,
    path: String,
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn run(mut request_rx: mpsc::Receiver<ServiceRequest>, state_tx: watch::Sender<Value>) {
    info!("bluetooth service started");
    loop {
        match connect_and_run(&mut request_rx, &state_tx).await {
            Ok(()) => break,
            Err(e) => {
                warn!(error = %e, "bluetooth D-Bus connection failed; retrying in 5 s");
                state_tx.send_replace(unavailable_snapshot());
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
    info!("bluetooth service stopped");
}

async fn connect_and_run(
    request_rx: &mut mpsc::Receiver<ServiceRequest>,
    state_tx: &watch::Sender<Value>,
) -> Result<(), BtError> {
    let conn = zbus::Connection::system().await?;
    let obj_mgr = zbus::fdo::ObjectManagerProxy::builder(&conn)
        .destination("org.bluez")?
        .path("/")?
        .build()
        .await?;

    let mut bt_state = scan_objects(&obj_mgr).await?;
    state_tx.send_replace(build_snapshot(&bt_state));

    let mut added = obj_mgr.receive_interfaces_added().await?;
    let mut removed = obj_mgr.receive_interfaces_removed().await?;
    let mut poll = tokio::time::interval(Duration::from_secs(5));
    poll.tick().await;

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                handle_request(req, &conn, &bt_state).await;
                // Re-scan after actions
                if let Ok(new_state) = scan_objects(&obj_mgr).await {
                    bt_state = new_state;
                    state_tx.send_replace(build_snapshot(&bt_state));
                }
            }
            signal = added.next() => {
                if signal.is_none() { break; }
                if let Ok(new_state) = scan_objects(&obj_mgr).await {
                    bt_state = new_state;
                    state_tx.send_replace(build_snapshot(&bt_state));
                }
            }
            signal = removed.next() => {
                if signal.is_none() { break; }
                if let Ok(new_state) = scan_objects(&obj_mgr).await {
                    bt_state = new_state;
                    state_tx.send_replace(build_snapshot(&bt_state));
                }
            }
            _ = poll.tick() => {
                if let Ok(new_state) = scan_objects(&obj_mgr).await {
                    bt_state = new_state;
                    state_tx.send_replace(build_snapshot(&bt_state));
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Object scanning
// ---------------------------------------------------------------------------

async fn scan_objects(obj_mgr: &zbus::fdo::ObjectManagerProxy<'_>) -> Result<BtState, BtError> {
    let objects = obj_mgr
        .get_managed_objects()
        .await
        .map_err(|e| BtError::Dbus(e.to_string()))?;

    let mut state = BtState {
        powered: false,
        discovering: false,
        adapter_path: None,
        devices: HashMap::new(),
    };

    for (path, ifaces) in &objects {
        if let Some(adapter_props) = ifaces.get("org.bluez.Adapter1") {
            state.adapter_path = Some(path.to_string());
            state.powered = get_owned_bool(adapter_props, "Powered");
            state.discovering = get_owned_bool(adapter_props, "Discovering");
        }

        if let Some(dev_props) = ifaces.get("org.bluez.Device1") {
            let address = get_owned_string(dev_props, "Address");
            let dev = BtDevice {
                address: address.clone(),
                name: get_owned_string(dev_props, "Name"),
                icon: get_owned_string(dev_props, "Icon"),
                paired: get_owned_bool(dev_props, "Paired"),
                connected: get_owned_bool(dev_props, "Connected"),
                trusted: get_owned_bool(dev_props, "Trusted"),
                battery: ifaces
                    .get("org.bluez.Battery1")
                    .and_then(|bp| get_owned_u8(bp, "Percentage"))
                    .map(|v| v as i64),
                path: path.to_string(),
            };
            state.devices.insert(address, dev);
        }
    }

    Ok(state)
}

fn get_owned_bool(props: &HashMap<String, OwnedValue>, key: &str) -> bool {
    props
        .get(key)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}

fn get_owned_string(props: &HashMap<String, OwnedValue>, key: &str) -> String {
    props
        .get(key)
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn get_owned_u8(props: &HashMap<String, OwnedValue>, key: &str) -> Option<u8> {
    props.get(key).and_then(|v| u8::try_from(v).ok())
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

fn build_snapshot(state: &BtState) -> Value {
    let devs: Vec<Value> = state.devices.values().map(device_to_value).collect();

    json_map([
        ("powered", Value::Bool(state.powered)),
        ("discovering", Value::Bool(state.discovering)),
        ("devices", Value::Array(devs)),
    ])
}

fn device_to_value(d: &BtDevice) -> Value {
    let bat = match d.battery {
        Some(v) => Value::from(v),
        None => Value::Null,
    };
    json_map([
        ("address", Value::from(d.address.as_str())),
        ("name", Value::from(d.name.as_str())),
        ("icon", Value::from(d.icon.as_str())),
        ("paired", Value::Bool(d.paired)),
        ("connected", Value::Bool(d.connected)),
        ("trusted", Value::Bool(d.trusted)),
        ("battery", bat),
    ])
}

// ---------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------

async fn handle_request(req: ServiceRequest, conn: &zbus::Connection, state: &BtState) {
    let result = match req.action.as_str() {
        "power" => handle_power(&req.payload, conn, state).await,
        "scan_start" => handle_scan_start(conn, state).await,
        "scan_stop" => handle_scan_stop(conn, state).await,
        "connect" => handle_device_action(&req.payload, conn, state, "Connect").await,
        "disconnect" => handle_device_action(&req.payload, conn, state, "Disconnect").await,
        "pair" => handle_device_action(&req.payload, conn, state, "Pair").await,
        "forget" => handle_forget(&req.payload, conn, state).await,
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    req.reply.send(result).ok();
}

async fn handle_power(
    payload: &Value,
    conn: &zbus::Connection,
    state: &BtState,
) -> Result<Value, ServiceError> {
    let on = extract_bool(payload, "on").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'on' bool field".to_string(),
    })?;
    let path = adapter_path(state)?;
    let proxy = adapter_proxy(conn, &path).await?;
    proxy
        .set_property("Powered", on)
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Null)
}

async fn handle_scan_start(
    conn: &zbus::Connection,
    state: &BtState,
) -> Result<Value, ServiceError> {
    let path = adapter_path(state)?;
    let proxy = adapter_proxy(conn, &path).await?;
    let _: () = proxy
        .call("StartDiscovery", &())
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Null)
}

async fn handle_scan_stop(conn: &zbus::Connection, state: &BtState) -> Result<Value, ServiceError> {
    let path = adapter_path(state)?;
    let proxy = adapter_proxy(conn, &path).await?;
    let _: () = proxy
        .call("StopDiscovery", &())
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Null)
}

async fn handle_device_action(
    payload: &Value,
    conn: &zbus::Connection,
    state: &BtState,
    method: &str,
) -> Result<Value, ServiceError> {
    let address = extract_str(payload, "address").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'address' field".to_string(),
    })?;
    let dev = find_device(state, address)?;
    let proxy = device_proxy(conn, &dev.path).await?;
    let _: () = proxy
        .call(method, &())
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Null)
}

async fn handle_forget(
    payload: &Value,
    conn: &zbus::Connection,
    state: &BtState,
) -> Result<Value, ServiceError> {
    let address = extract_str(payload, "address").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'address' field".to_string(),
    })?;
    let dev = find_device(state, address)?;
    let adapter = adapter_path(state)?;
    let proxy = adapter_proxy(conn, &adapter).await?;
    let dev_path = zbus::zvariant::ObjectPath::try_from(dev.path.as_str())
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    let _: () = proxy
        .call("RemoveDevice", &(dev_path,))
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Null)
}

// ---------------------------------------------------------------------------
// Proxy helpers
// ---------------------------------------------------------------------------

fn adapter_path(state: &BtState) -> Result<String, ServiceError> {
    state.adapter_path.clone().ok_or(ServiceError::Unavailable)
}

fn find_device<'a>(state: &'a BtState, address: &str) -> Result<&'a BtDevice, ServiceError> {
    state
        .devices
        .get(address)
        .ok_or_else(|| ServiceError::ActionPayload {
            msg: format!("device '{address}' not found"),
        })
}

async fn adapter_proxy<'a>(
    conn: &'a zbus::Connection,
    path: &'a str,
) -> Result<zbus::Proxy<'a>, ServiceError> {
    zbus::Proxy::new(conn, "org.bluez", path, "org.bluez.Adapter1")
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })
}

async fn device_proxy<'a>(
    conn: &'a zbus::Connection,
    path: &'a str,
) -> Result<zbus::Proxy<'a>, ServiceError> {
    zbus::Proxy::new(conn, "org.bluez", path, "org.bluez.Device1")
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })
}

// ---------------------------------------------------------------------------
// Payload helpers
// ---------------------------------------------------------------------------

fn extract_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.as_object()?.get(key)?.as_str()
}

fn extract_bool(v: &Value, key: &str) -> Option<bool> {
    v.as_object()?.get(key)?.as_bool()
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum BtError {
    #[error("zbus error: {0}")]
    Zbus(#[from] zbus::Error),
    #[error("D-Bus error: {0}")]
    Dbus(String),
}
