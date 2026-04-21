// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `bluetooth` service — BlueZ D-Bus backend.

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{message::Type as MessageType, MatchRule, MessageStream};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::json_map;

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
        ("available", Value::Bool(false)),
        ("powered", Value::Bool(false)),
        ("discovering", Value::Bool(false)),
        ("devices", Value::Array(vec![])),
    ])
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct BtState {
    adapter_path: Option<String>,
    adapters: HashMap<String, BtAdapter>,
    devices: HashMap<String, BtDevice>,
}

#[derive(Clone, Default)]
struct BtAdapter {
    powered: bool,
    discovering: bool,
}

#[derive(Clone)]
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

impl BtDevice {
    fn placeholder(path: &str) -> Self {
        Self {
            address: String::new(),
            name: String::new(),
            icon: String::new(),
            paired: false,
            connected: false,
            trusted: false,
            battery: None,
            path: path.to_string(),
        }
    }
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
    let (action_done_tx, mut action_done_rx) = mpsc::unbounded_channel::<()>();

    let mut added = signal_stream(
        &conn,
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("org.bluez")?
            .interface("org.freedesktop.DBus.ObjectManager")?
            .member("InterfacesAdded")?
            .path("/")?
            .build(),
    )
    .await?;
    let mut removed = signal_stream(
        &conn,
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("org.bluez")?
            .interface("org.freedesktop.DBus.ObjectManager")?
            .member("InterfacesRemoved")?
            .path("/")?
            .build(),
    )
    .await?;
    let mut changed = signal_stream(
        &conn,
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("org.bluez")?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .path_namespace("/org/bluez")?
            .build(),
    )
    .await?;

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                let conn = conn.clone();
                let state_snapshot = bt_state.clone();
                let action_done_tx = action_done_tx.clone();
                tokio::spawn(async move {
                    handle_request(req, &conn, &state_snapshot).await;
                    action_done_tx.send(()).ok();
                });
            }
            action_done = action_done_rx.recv() => {
                let Some(()) = action_done else { break };
                // Reconcile local state once long-running BlueZ actions complete.
                if let Ok(new_state) = scan_objects(&obj_mgr).await {
                    bt_state = new_state;
                    state_tx.send_replace(build_snapshot(&bt_state));
                }
            }
            signal = added.next() => {
                let Some(signal) = signal else { break };
                let msg = signal?;
                apply_interfaces_added_message(&mut bt_state, &msg)?;
                state_tx.send_replace(build_snapshot(&bt_state));
            }
            signal = removed.next() => {
                let Some(signal) = signal else { break };
                let msg = signal?;
                apply_interfaces_removed_message(&mut bt_state, &msg)?;
                state_tx.send_replace(build_snapshot(&bt_state));
            }
            signal = changed.next() => {
                let Some(signal) = signal else { break };
                let msg = signal?;
                apply_properties_changed_message(&mut bt_state, &msg)?;
                state_tx.send_replace(build_snapshot(&bt_state));
            }
        }
    }
    Ok(())
}

async fn signal_stream(
    conn: &zbus::Connection,
    rule: MatchRule<'static>,
) -> Result<MessageStream, BtError> {
    MessageStream::for_match_rule(rule, conn, None)
        .await
        .map_err(BtError::Zbus)
}

// ---------------------------------------------------------------------------
// Object scanning
// ---------------------------------------------------------------------------

async fn scan_objects(obj_mgr: &zbus::fdo::ObjectManagerProxy<'_>) -> Result<BtState, BtError> {
    let objects = obj_mgr
        .get_managed_objects()
        .await
        .map_err(|e| BtError::Dbus(e.to_string()))?;

    let mut state = BtState::default();

    for (path, ifaces) in &objects {
        let path = path.to_string();
        for (iface, props) in ifaces {
            apply_interface_properties(&mut state, &path, iface.as_str(), props);
        }
    }

    sync_active_adapter(&mut state);
    Ok(state)
}

fn apply_interfaces_added_message(state: &mut BtState, msg: &zbus::Message) -> Result<(), BtError> {
    let (path, ifaces): (
        OwnedObjectPath,
        HashMap<String, HashMap<String, OwnedValue>>,
    ) = msg
        .body()
        .deserialize()
        .map_err(|e| BtError::Dbus(e.to_string()))?;
    let path = path.to_string();
    for (iface, props) in ifaces {
        apply_interface_properties(state, &path, &iface, &props);
    }
    sync_active_adapter(state);
    Ok(())
}

fn apply_interfaces_removed_message(
    state: &mut BtState,
    msg: &zbus::Message,
) -> Result<(), BtError> {
    let (path, ifaces): (OwnedObjectPath, Vec<String>) = msg
        .body()
        .deserialize()
        .map_err(|e| BtError::Dbus(e.to_string()))?;
    let path = path.to_string();
    for iface in ifaces {
        remove_interface(state, &path, &iface);
    }
    sync_active_adapter(state);
    Ok(())
}

fn apply_properties_changed_message(
    state: &mut BtState,
    msg: &zbus::Message,
) -> Result<(), BtError> {
    let path = msg
        .header()
        .path()
        .ok_or_else(|| BtError::Dbus("PropertiesChanged without object path".to_string()))?
        .to_string();
    let (iface, changed, invalidated): (String, HashMap<String, OwnedValue>, Vec<String>) = msg
        .body()
        .deserialize()
        .map_err(|e| BtError::Dbus(e.to_string()))?;
    apply_interface_change(state, &path, &iface, &changed, &invalidated);
    sync_active_adapter(state);
    Ok(())
}

fn apply_interface_properties(
    state: &mut BtState,
    path: &str,
    iface: &str,
    props: &HashMap<String, OwnedValue>,
) {
    match iface {
        "org.bluez.Adapter1" => apply_adapter_props(state, path, props),
        "org.bluez.Device1" => apply_device_props(state, path, props),
        "org.bluez.Battery1" => apply_battery_props(state, path, props),
        _ => {}
    }
}

fn apply_interface_change(
    state: &mut BtState,
    path: &str,
    iface: &str,
    changed: &HashMap<String, OwnedValue>,
    invalidated: &[String],
) {
    match iface {
        "org.bluez.Adapter1" => apply_adapter_change(state, path, changed, invalidated),
        "org.bluez.Device1" => apply_device_change(state, path, changed, invalidated),
        "org.bluez.Battery1" => apply_battery_change(state, path, changed, invalidated),
        _ => {}
    }
}

fn apply_adapter_props(state: &mut BtState, path: &str, props: &HashMap<String, OwnedValue>) {
    let adapter = state.adapters.entry(path.to_string()).or_default();
    if let Some(powered) = owned_prop::<bool>(props, "Powered") {
        adapter.powered = powered;
    }
    if let Some(discovering) = owned_prop::<bool>(props, "Discovering") {
        adapter.discovering = discovering;
    }
    if state.adapter_path.is_none() {
        state.adapter_path = Some(path.to_string());
    }
}

fn apply_device_props(state: &mut BtState, path: &str, props: &HashMap<String, OwnedValue>) {
    let device = state
        .devices
        .entry(path.to_string())
        .or_insert_with(|| BtDevice::placeholder(path));
    if let Some(address) = owned_prop::<String>(props, "Address") {
        device.address = address;
    }
    if let Some(name) = device_name(props) {
        device.name = name;
    }
    if let Some(icon) = owned_prop::<String>(props, "Icon") {
        device.icon = icon;
    }
    if let Some(paired) = owned_prop::<bool>(props, "Paired") {
        device.paired = paired;
    }
    if let Some(connected) = owned_prop::<bool>(props, "Connected") {
        device.connected = connected;
    }
    if let Some(trusted) = owned_prop::<bool>(props, "Trusted") {
        device.trusted = trusted;
    }
}

fn apply_battery_props(state: &mut BtState, path: &str, props: &HashMap<String, OwnedValue>) {
    let device = state
        .devices
        .entry(path.to_string())
        .or_insert_with(|| BtDevice::placeholder(path));
    if let Some(percentage) = owned_prop::<u8>(props, "Percentage") {
        device.battery = Some(i64::from(percentage));
    }
}

fn apply_adapter_change(
    state: &mut BtState,
    path: &str,
    changed: &HashMap<String, OwnedValue>,
    invalidated: &[String],
) {
    apply_adapter_props(state, path, changed);
    let adapter = state.adapters.entry(path.to_string()).or_default();
    for key in invalidated {
        match key.as_str() {
            "Powered" => adapter.powered = false,
            "Discovering" => adapter.discovering = false,
            _ => {}
        }
    }
}

fn apply_device_change(
    state: &mut BtState,
    path: &str,
    changed: &HashMap<String, OwnedValue>,
    invalidated: &[String],
) {
    apply_device_props(state, path, changed);
    let device = state
        .devices
        .entry(path.to_string())
        .or_insert_with(|| BtDevice::placeholder(path));
    for key in invalidated {
        match key.as_str() {
            "Address" => device.address.clear(),
            "Name" | "Alias" => device.name.clear(),
            "Icon" => device.icon.clear(),
            "Paired" => device.paired = false,
            "Connected" => device.connected = false,
            "Trusted" => device.trusted = false,
            _ => {}
        }
    }
}

fn apply_battery_change(
    state: &mut BtState,
    path: &str,
    changed: &HashMap<String, OwnedValue>,
    invalidated: &[String],
) {
    apply_battery_props(state, path, changed);
    if invalidated.iter().any(|key| key == "Percentage") {
        let device = state
            .devices
            .entry(path.to_string())
            .or_insert_with(|| BtDevice::placeholder(path));
        device.battery = None;
    }
}

fn remove_interface(state: &mut BtState, path: &str, iface: &str) {
    match iface {
        "org.bluez.Adapter1" => {
            state.adapters.remove(path);
            if state.adapter_path.as_deref() == Some(path) {
                state.adapter_path = None;
            }
        }
        "org.bluez.Device1" => {
            state.devices.remove(path);
        }
        "org.bluez.Battery1" => {
            if let Some(device) = state.devices.get_mut(path) {
                device.battery = None;
            }
        }
        _ => {}
    }
}

fn sync_active_adapter(state: &mut BtState) {
    let current_valid = state
        .adapter_path
        .as_ref()
        .map(|path| state.adapters.contains_key(path))
        .unwrap_or(false);
    if current_valid {
        return;
    }
    state.adapter_path = state.adapters.keys().min().cloned();
}

fn device_name(props: &HashMap<String, OwnedValue>) -> Option<String> {
    owned_prop::<String>(props, "Name").or_else(|| owned_prop::<String>(props, "Alias"))
}

fn owned_prop<T>(props: &HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    props
        .get(key)
        .and_then(|value| T::try_from(value.clone()).ok())
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

fn build_snapshot(state: &BtState) -> Value {
    let devs: Vec<Value> = state
        .devices
        .values()
        .filter(|device| !device.address.is_empty())
        .map(device_to_value)
        .collect();

    let adapter = state
        .adapter_path
        .as_ref()
        .and_then(|path| state.adapters.get(path));

    json_map([
        ("available", Value::Bool(!state.adapters.is_empty())),
        (
            "powered",
            Value::Bool(adapter.map(|adapter| adapter.powered).unwrap_or(false)),
        ),
        (
            "discovering",
            Value::Bool(adapter.map(|adapter| adapter.discovering).unwrap_or(false)),
        ),
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
    let (path, adapter) = active_adapter(state)?;
    if adapter.powered == on {
        return Ok(Value::Null);
    }
    let proxy = adapter_proxy(conn, &path).await?;
    tokio::time::timeout(Duration::from_secs(5), proxy.set_property("Powered", on))
        .await
        .map_err(|_| ServiceError::Internal {
            msg: "bluetooth power request timed out".to_string(),
        })?
        .map_err(|e| {
            map_bluetooth_action_error(
                if on {
                    "turn bluetooth on"
                } else {
                    "turn bluetooth off"
                },
                e.into(),
            )
        })?;
    Ok(Value::Null)
}

async fn handle_scan_start(
    conn: &zbus::Connection,
    state: &BtState,
) -> Result<Value, ServiceError> {
    let (path, adapter) = active_adapter(state)?;
    if !adapter.powered {
        return Err(ServiceError::Internal {
            msg: "bluetooth is off".to_string(),
        });
    }
    if adapter.discovering {
        return Ok(Value::Null);
    }
    let proxy = adapter_proxy(conn, &path).await?;
    let _: () = tokio::time::timeout(Duration::from_secs(5), proxy.call("StartDiscovery", &()))
        .await
        .map_err(|_| ServiceError::Internal {
            msg: "bluetooth scan start timed out".to_string(),
        })?
        .map_err(|e| map_bluetooth_action_error("start bluetooth scan", e))?;
    Ok(Value::Null)
}

async fn handle_scan_stop(conn: &zbus::Connection, state: &BtState) -> Result<Value, ServiceError> {
    let (path, adapter) = active_adapter(state)?;
    if !adapter.powered {
        return Ok(Value::Null);
    }
    if !adapter.discovering {
        return Ok(Value::Null);
    }
    let proxy = adapter_proxy(conn, &path).await?;
    let _: () = tokio::time::timeout(Duration::from_secs(5), proxy.call("StopDiscovery", &()))
        .await
        .map_err(|_| ServiceError::Internal {
            msg: "bluetooth scan stop timed out".to_string(),
        })?
        .map_err(|e| map_bluetooth_action_error("stop bluetooth scan", e))?;
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
    if method != "Disconnect" {
        let (_, adapter) = active_adapter(state)?;
        if !adapter.powered {
            return Err(ServiceError::Internal {
                msg: "bluetooth is off".to_string(),
            });
        }
    }
    match method {
        "Connect" if dev.connected => return Ok(Value::Null),
        "Disconnect" if !dev.connected => return Ok(Value::Null),
        "Pair" if dev.paired => return Ok(Value::Null),
        _ => {}
    }
    let proxy = device_proxy(conn, &dev.path).await?;
    let _: () = tokio::time::timeout(Duration::from_secs(20), proxy.call(method, &()))
        .await
        .map_err(|_| ServiceError::Internal {
            msg: format!("bluetooth {method} request timed out"),
        })?
        .map_err(|e| {
            map_bluetooth_action_error(
                match method {
                    "Connect" => "connect bluetooth device",
                    "Disconnect" => "disconnect bluetooth device",
                    "Pair" => "pair bluetooth device",
                    _ => "complete bluetooth action",
                },
                e,
            )
        })?;
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
    let _: () = tokio::time::timeout(
        Duration::from_secs(10),
        proxy.call("RemoveDevice", &(dev_path,)),
    )
    .await
    .map_err(|_| ServiceError::Internal {
        msg: "bluetooth forget request timed out".to_string(),
    })?
    .map_err(|e| map_bluetooth_action_error("forget bluetooth device", e))?;
    Ok(Value::Null)
}

// ---------------------------------------------------------------------------
// Proxy helpers
// ---------------------------------------------------------------------------

fn adapter_path(state: &BtState) -> Result<String, ServiceError> {
    state.adapter_path.clone().ok_or(ServiceError::Unavailable)
}

fn active_adapter(state: &BtState) -> Result<(String, &BtAdapter), ServiceError> {
    let path = adapter_path(state)?;
    let adapter = state.adapters.get(&path).ok_or(ServiceError::Unavailable)?;
    Ok((path, adapter))
}

fn find_device<'a>(state: &'a BtState, address: &str) -> Result<&'a BtDevice, ServiceError> {
    state
        .devices
        .values()
        .find(|device| device.address == address)
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

fn map_bluetooth_action_error(context: &str, err: zbus::Error) -> ServiceError {
    warn!(context, error = %err, error_debug = ?err, "bluetooth action failed");

    let msg = match &err {
        zbus::Error::MethodError(name, detail, _) => {
            method_error_message(context, name.as_str(), detail.as_deref())
        }
        zbus::Error::FDO(fdo) => fdo_error_message(context, fdo),
        zbus::Error::Failure(detail) => generic_bluetooth_error(context, Some(detail.as_str())),
        _ => format!("{context} failed: {err}"),
    };

    ServiceError::Internal { msg }
}

fn fdo_error_message(context: &str, err: &zbus::fdo::Error) -> String {
    match err {
        zbus::fdo::Error::Failed(detail) => generic_bluetooth_error(context, Some(detail.as_str())),
        zbus::fdo::Error::NoReply(_) | zbus::fdo::Error::TimedOut(_) | zbus::fdo::Error::Timeout(_) => {
            format!("{context} timed out")
        }
        zbus::fdo::Error::UnknownObject(_) => "bluetooth device is no longer available".to_string(),
        zbus::fdo::Error::UnknownInterface(_) => "bluetooth backend interface is unavailable".to_string(),
        zbus::fdo::Error::NotSupported(detail) => {
            let detail = clean_bluetooth_error_detail(Some(detail.as_str()));
            detail.unwrap_or_else(|| format!("{context} is not supported on this system"))
        }
        other => generic_bluetooth_error(context, Some(&other.to_string())),
    }
}

fn method_error_message(context: &str, name: &str, detail: Option<&str>) -> String {
    match name {
        "org.bluez.Error.NotReady" => "bluetooth adapter is not ready".to_string(),
        "org.bluez.Error.NotAvailable" => "bluetooth adapter is unavailable".to_string(),
        "org.bluez.Error.NotPowered" => "bluetooth is off".to_string(),
        "org.bluez.Error.InProgress" => format!("{context} is already in progress"),
        "org.bluez.Error.AlreadyConnected" => "bluetooth device is already connected".to_string(),
        "org.bluez.Error.NotConnected" => "bluetooth device is not connected".to_string(),
        "org.bluez.Error.AlreadyExists" => "bluetooth device is already paired".to_string(),
        "org.freedesktop.DBus.Error.UnknownObject" => "bluetooth device is no longer available".to_string(),
        "org.freedesktop.DBus.Error.UnknownInterface" => "bluetooth backend interface is unavailable".to_string(),
        "org.bluez.Error.Failed" | "org.freedesktop.zbus.Error" => {
            generic_bluetooth_error(context, detail)
        }
        _ => {
            if let Some(detail) = clean_bluetooth_error_detail(detail) {
                format!("{context} failed: {detail}")
            } else {
                format!("{context} failed ({name})")
            }
        }
    }
}

fn generic_bluetooth_error(context: &str, detail: Option<&str>) -> String {
    if let Some(detail) = clean_bluetooth_error_detail(detail) {
        format!("{context} failed: {detail}")
    } else {
        format!("{context} failed")
    }
}

fn clean_bluetooth_error_detail(detail: Option<&str>) -> Option<String> {
    let detail = detail?.trim();
    if detail.is_empty() {
        return None;
    }
    if matches!(detail, "Failed" | "org.freedesktop.zbus.Error: Failed") {
        return None;
    }
    Some(detail.to_string())
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
