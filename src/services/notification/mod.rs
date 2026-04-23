// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `notification` service — freedesktop Notifications D-Bus server.
//!
//! ## Architectural exception
//!
//! This service uses `Arc<tokio::sync::RwLock<NotifState>>` to share state
//! between the zbus D-Bus server (whose method handlers execute in zbus's
//! internal context) and the service's tokio task that handles IPC requests.
//! This is the minimal exception to the no-shared-state rule, justified by
//! the D-Bus server / service task boundary.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{broadcast, mpsc, watch, RwLock};
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::services::applications::{AppLookup, AppResolver, ResolvedApp};
use crate::session_env;
use crate::util::{json_map, unix_now_ms};

const NOTIFICATION_DBUS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATION_DBUS_PATH: &str = "/org/freedesktop/Notifications";
const NOTIFICATION_SPEC_VERSION: &str = "1.2";
const NOTIFICATION_SERVER_VENDOR: &str = env!("CARGO_PKG_NAME");
const NOTIFICATION_SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const NOTIFICATION_CAPABILITIES: &[&str] = &["body", "actions", "persistence"];

fn notification_server_name() -> &'static str {
    option_env!("CARGO_BIN_NAME").unwrap_or("qsovd")
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Spawn the `notification` service and return its [`ServiceHandle`].
pub fn spawn(_cfg: &Config, apps: Arc<AppResolver>) -> ServiceHandle {
    let initial = empty_snapshot();
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);
    let (events_tx, _) = broadcast::channel(64);

    tokio::spawn(run(request_rx, state_tx.clone(), events_tx.clone(), apps));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: Some(events_tx),
    }
}

fn empty_snapshot() -> Value {
    json_map([
        ("unread_count", Value::from(0_i64)),
        ("history", Value::Array(vec![])),
    ])
}

// ---------------------------------------------------------------------------
// Notification data
// ---------------------------------------------------------------------------

struct NotifState {
    notifications: Vec<Notification>,
    next_id: u32,
}

impl Default for NotifState {
    fn default() -> Self {
        Self {
            notifications: Vec::new(),
            next_id: 1,
        }
    }
}

struct Notification {
    id: u32,
    app_name: String,
    summary: String,
    body: String,
    icon: String,
    urgency: String,
    timestamp: i64,
    actions: Vec<NotifAction>,
    read: bool,
}

struct NotifAction {
    id: String,
    label: String,
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    state_tx: watch::Sender<Value>,
    events_tx: broadcast::Sender<Value>,
    apps: Arc<AppResolver>,
) {
    info!("notification service started");

    let shared = Arc::new(RwLock::new(NotifState::default()));

    // Start the D-Bus server
    let dbus_conn =
        match start_dbus_server(shared.clone(), state_tx.clone(), events_tx.clone(), apps).await {
            Ok(conn) => Some(conn),
            Err(e) => {
                warn!(error = %e, "notification D-Bus server failed to start");
                None
            }
        };

    // Handle IPC requests
    while let Some(req) = request_rx.recv().await {
        handle_request(req, &shared, &state_tx, &events_tx, dbus_conn.as_ref()).await;
    }

    info!("notification service stopped");
}

// ---------------------------------------------------------------------------
// D-Bus server
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct NotifServer {
    shared: Arc<RwLock<NotifState>>,
    state_tx: watch::Sender<Value>,
    events_tx: broadcast::Sender<Value>,
    apps: Arc<AppResolver>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
#[allow(clippy::too_many_arguments)]
impl NotifServer {
    async fn get_capabilities(&self) -> Vec<String> {
        NOTIFICATION_CAPABILITIES
            .iter()
            .map(|capability| (*capability).to_string())
            .collect()
    }

    async fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::Value<'_>>,
        _expire_timeout: i32,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> u32 {
        let urgency = parse_urgency(&hints);
        let parsed_actions = parse_actions(&actions);
        let resolved =
            resolve_notification_app(&self.apps, connection, &header, app_name, app_icon, &hints)
                .await;
        let (app_name, summary) =
            normalize_notification_text(app_name, summary, resolved.display_name.as_str());

        let mut state = self.shared.write().await;
        let id = if replaces_id > 0 {
            let data = NotifData {
                app_name: &app_name,
                summary: &summary,
                body,
                icon: resolved.icon.as_str(),
                urgency: &urgency,
                actions: parsed_actions,
            };
            replace_notification(&mut state, replaces_id, data);
            replaces_id
        } else {
            let data = NotifData {
                app_name: &app_name,
                summary: &summary,
                body,
                icon: resolved.icon.as_str(),
                urgency: &urgency,
                actions: parsed_actions,
            };
            add_notification(&mut state, data)
        };

        let snap = state_to_snapshot(&state);
        self.state_tx.send_replace(snap);

        let event = json_map([
            ("type", Value::from("new")),
            ("id", Value::from(id as i64)),
            ("app_name", Value::from(app_name)),
            ("summary", Value::from(summary)),
        ]);
        let _ = self.events_tx.send(event);

        id
    }

    async fn close_notification(&self, id: u32) {
        let mut state = self.shared.write().await;
        state.notifications.retain(|n| n.id != id);
        let snap = state_to_snapshot(&state);
        self.state_tx.send_replace(snap);

        let event = json_map([
            ("type", Value::from("closed")),
            ("id", Value::from(id as i64)),
            ("reason", Value::from(3_i64)), // dismissed by call
        ]);
        let _ = self.events_tx.send(event);
    }

    async fn get_server_information(&self) -> (String, String, String, String) {
        (
            notification_server_name().to_string(),
            NOTIFICATION_SERVER_VENDOR.to_string(),
            NOTIFICATION_SERVER_VERSION.to_string(),
            NOTIFICATION_SPEC_VERSION.to_string(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        action_key: String,
    ) -> zbus::Result<()>;
}

async fn start_dbus_server(
    shared: Arc<RwLock<NotifState>>,
    state_tx: watch::Sender<Value>,
    events_tx: broadcast::Sender<Value>,
    apps: Arc<AppResolver>,
) -> Result<zbus::Connection, NotifError> {
    let server = NotifServer {
        shared,
        state_tx,
        events_tx,
        apps,
    };

    let mut last_err = None;

    for candidate in session_env::session_bus_candidates() {
        match zbus::connection::Builder::address(candidate.value.as_str()) {
            Ok(builder) => {
                let builder = builder
                    .name(NOTIFICATION_DBUS_NAME)?
                    .serve_at(NOTIFICATION_DBUS_PATH, server.clone())?;
                match builder.build().await {
                    Ok(conn) => return Ok(conn),
                    Err(err) => {
                        warn!(
                            source = candidate.source,
                            address = %candidate.value,
                            error = %err,
                            "notification session bus candidate failed"
                        );
                        last_err = Some(err);
                    }
                }
            }
            Err(err) => {
                warn!(
                    source = candidate.source,
                    address = %candidate.value,
                    error = %err,
                    "notification session bus candidate could not be parsed"
                );
                last_err = Some(err);
            }
        }
    }

    Err(NotifError::from(last_err.unwrap_or_else(|| {
        zbus::Error::Failure("no usable session bus candidate found".to_string())
    })))
}

// ---------------------------------------------------------------------------
// Notification management helpers
// ---------------------------------------------------------------------------

struct NotifData<'a> {
    app_name: &'a str,
    summary: &'a str,
    body: &'a str,
    icon: &'a str,
    urgency: &'a str,
    actions: Vec<NotifAction>,
}

fn add_notification(state: &mut NotifState, data: NotifData<'_>) -> u32 {
    let id = state.next_id;
    state.next_id += 1;
    state.notifications.push(Notification {
        id,
        app_name: data.app_name.to_string(),
        summary: data.summary.to_string(),
        body: data.body.to_string(),
        icon: data.icon.to_string(),
        urgency: data.urgency.to_string(),
        timestamp: unix_now_ms(),
        actions: data.actions,
        read: false,
    });
    id
}

fn replace_notification(state: &mut NotifState, id: u32, data: NotifData<'_>) {
    if let Some(n) = state.notifications.iter_mut().find(|n| n.id == id) {
        n.app_name = data.app_name.to_string();
        n.summary = data.summary.to_string();
        n.body = data.body.to_string();
        n.icon = data.icon.to_string();
        n.urgency = data.urgency.to_string();
        n.timestamp = unix_now_ms();
        n.actions = data.actions;
        n.read = false;
    } else {
        state.notifications.push(Notification {
            id,
            app_name: data.app_name.to_string(),
            summary: data.summary.to_string(),
            body: data.body.to_string(),
            icon: data.icon.to_string(),
            urgency: data.urgency.to_string(),
            timestamp: unix_now_ms(),
            actions: data.actions,
            read: false,
        });
    }
}

fn parse_urgency(hints: &HashMap<String, zbus::zvariant::Value<'_>>) -> String {
    if let Some(val) = hints.get("urgency") {
        let level: u8 = match val {
            zbus::zvariant::Value::U8(v) => *v,
            _ => 1,
        };
        return match level {
            0 => "low",
            2 => "critical",
            _ => "normal",
        }
        .to_string();
    }
    "normal".to_string()
}

fn parse_actions(actions: &[String]) -> Vec<NotifAction> {
    actions
        .chunks(2)
        .filter_map(|chunk| {
            if chunk.len() == 2 {
                Some(NotifAction {
                    id: chunk[0].clone(),
                    label: chunk[1].clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn normalize_notification_text(
    app_name: &str,
    summary: &str,
    resolved_name: &str,
) -> (String, String) {
    let app_name = app_name.trim();
    let summary = summary.trim();
    let resolved_name = resolved_name.trim();

    if !resolved_name.is_empty() {
        return (resolved_name.to_string(), summary.to_string());
    }

    if !app_name.is_empty() {
        return (app_name.to_string(), summary.to_string());
    }

    if !summary.is_empty() {
        return (summary.to_string(), String::new());
    }

    (String::new(), String::new())
}

async fn resolve_notification_app(
    apps: &AppResolver,
    connection: &zbus::Connection,
    header: &zbus::message::Header<'_>,
    app_name: &str,
    app_icon: &str,
    hints: &HashMap<String, zbus::zvariant::Value<'_>>,
) -> ResolvedApp {
    let process_id = hint_u32(hints, "sender-pid").or_else(|| hint_u32(hints, "x-kde-app-pid"));
    let process_id = match process_id {
        Some(process_id) => Some(process_id),
        None => sender_process_id(connection, header).await,
    };

    let icon_hint = [
        hint_string(hints, "image-path"),
        hint_string(hints, "image_path"),
        non_empty(app_icon),
    ]
    .into_iter()
    .flatten()
    .next();

    apps.resolve(&AppLookup {
        icon_hint,
        desktop_entry: hint_string(hints, "desktop-entry"),
        app_name: non_empty(app_name),
        process_id,
        ..AppLookup::default()
    })
}

async fn sender_process_id(
    connection: &zbus::Connection,
    header: &zbus::message::Header<'_>,
) -> Option<u32> {
    let sender = header.sender()?.to_owned();
    let proxy = zbus::fdo::DBusProxy::new(connection).await.ok()?;
    proxy
        .get_connection_unix_process_id(sender.into())
        .await
        .ok()
}

fn hint_string(hints: &HashMap<String, zbus::zvariant::Value<'_>>, key: &str) -> Option<String> {
    match hints.get(key)? {
        zbus::zvariant::Value::Str(value) => non_empty(value.as_str()),
        zbus::zvariant::Value::Value(value) => match value.as_ref() {
            zbus::zvariant::Value::Str(value) => non_empty(value.as_str()),
            _ => None,
        },
        _ => None,
    }
}

fn hint_u32(hints: &HashMap<String, zbus::zvariant::Value<'_>>, key: &str) -> Option<u32> {
    match hints.get(key)? {
        zbus::zvariant::Value::U32(value) => Some(*value),
        zbus::zvariant::Value::I32(value) => (*value >= 0).then_some(*value as u32),
        zbus::zvariant::Value::U64(value) => u32::try_from(*value).ok(),
        zbus::zvariant::Value::I64(value) => u32::try_from(*value).ok(),
        zbus::zvariant::Value::Value(value) => match value.as_ref() {
            zbus::zvariant::Value::U32(value) => Some(*value),
            zbus::zvariant::Value::I32(value) => (*value >= 0).then_some(*value as u32),
            zbus::zvariant::Value::U64(value) => u32::try_from(*value).ok(),
            zbus::zvariant::Value::I64(value) => u32::try_from(*value).ok(),
            _ => None,
        },
        _ => None,
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ---------------------------------------------------------------------------
// Snapshot builder
// ---------------------------------------------------------------------------

fn state_to_snapshot(state: &NotifState) -> Value {
    let unread = state.notifications.iter().filter(|n| !n.read).count() as i64;
    let history: Vec<Value> = state
        .notifications
        .iter()
        .rev()
        .map(notif_to_value)
        .collect();
    json_map([
        ("unread_count", Value::from(unread)),
        ("history", Value::Array(history)),
    ])
}

fn notif_to_value(n: &Notification) -> Value {
    let actions: Vec<Value> = n
        .actions
        .iter()
        .map(|a| {
            json_map([
                ("id", Value::from(a.id.as_str())),
                ("label", Value::from(a.label.as_str())),
            ])
        })
        .collect();
    json_map([
        ("id", Value::from(n.id as i64)),
        ("app_name", Value::from(n.app_name.as_str())),
        ("summary", Value::from(n.summary.as_str())),
        ("body", Value::from(n.body.as_str())),
        ("icon", Value::from(n.icon.as_str())),
        ("urgency", Value::from(n.urgency.as_str())),
        ("timestamp", Value::from(n.timestamp)),
        ("actions", Value::Array(actions)),
    ])
}

// ---------------------------------------------------------------------------
// IPC request handling
// ---------------------------------------------------------------------------

async fn handle_request(
    req: ServiceRequest,
    shared: &Arc<RwLock<NotifState>>,
    state_tx: &watch::Sender<Value>,
    events_tx: &broadcast::Sender<Value>,
    dbus_conn: Option<&zbus::Connection>,
) {
    let result = match req.action.as_str() {
        "invoke_action" => handle_invoke_action(&req.payload, shared, events_tx, dbus_conn).await,
        "dismiss" => handle_dismiss(&req.payload, shared, state_tx, events_tx, dbus_conn).await,
        "dismiss_all" => handle_dismiss_all(shared, state_tx).await,
        "mark_read" => handle_mark_read(&req.payload, shared, state_tx).await,
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    req.reply.send(result).ok();
}

async fn handle_invoke_action(
    payload: &Value,
    shared: &Arc<RwLock<NotifState>>,
    events_tx: &broadcast::Sender<Value>,
    dbus_conn: Option<&zbus::Connection>,
) -> Result<Value, ServiceError> {
    let id = extract_u64(payload, "id").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'id' field".to_string(),
    })? as u32;
    let action_id =
        extract_str(payload, "action_id").ok_or_else(|| ServiceError::ActionPayload {
            msg: "missing 'action_id' field".to_string(),
        })?;
    let state = shared.read().await;
    let Some(notification) = state.notifications.iter().find(|n| n.id == id) else {
        return Err(ServiceError::ActionPayload {
            msg: format!("notification {id} not found"),
        });
    };
    if !notification.actions.iter().any(|a| a.id == action_id) {
        return Err(ServiceError::ActionPayload {
            msg: format!("notification {id} action '{action_id}' not found"),
        });
    }
    drop(state);

    let conn = dbus_conn.ok_or(ServiceError::Unavailable)?;
    let emitter = zbus::object_server::SignalEmitter::new(conn, "/org/freedesktop/Notifications")
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    NotifServer::action_invoked(&emitter, id, action_id.to_string())
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;

    let event = json_map([
        ("type", Value::from("action_invoked")),
        ("id", Value::from(id as i64)),
        ("action_id", Value::from(action_id)),
    ]);
    let _ = events_tx.send(event);
    Ok(Value::Null)
}

async fn handle_dismiss(
    payload: &Value,
    shared: &Arc<RwLock<NotifState>>,
    state_tx: &watch::Sender<Value>,
    events_tx: &broadcast::Sender<Value>,
    dbus_conn: Option<&zbus::Connection>,
) -> Result<Value, ServiceError> {
    let id = extract_u64(payload, "id").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'id' field".to_string(),
    })? as u32;
    let mut state = shared.write().await;
    state.notifications.retain(|n| n.id != id);
    state_tx.send_replace(state_to_snapshot(&state));

    let event = json_map([
        ("type", Value::from("closed")),
        ("id", Value::from(id as i64)),
        ("reason", Value::from(2_i64)), // dismissed by user
    ]);
    let _ = events_tx.send(event);

    if let Some(conn) = dbus_conn {
        if let Ok(emitter) = zbus::object_server::SignalEmitter::new(conn, NOTIFICATION_DBUS_PATH) {
            let _ = NotifServer::notification_closed(&emitter, id, 2).await;
        }
    }
    Ok(Value::Null)
}

async fn handle_dismiss_all(
    shared: &Arc<RwLock<NotifState>>,
    state_tx: &watch::Sender<Value>,
) -> Result<Value, ServiceError> {
    let mut state = shared.write().await;
    state.notifications.clear();
    state_tx.send_replace(state_to_snapshot(&state));
    Ok(Value::Null)
}

async fn handle_mark_read(
    payload: &Value,
    shared: &Arc<RwLock<NotifState>>,
    state_tx: &watch::Sender<Value>,
) -> Result<Value, ServiceError> {
    let mut state = shared.write().await;
    if let Some(id) = extract_u64(payload, "id") {
        let id = id as u32;
        if let Some(n) = state.notifications.iter_mut().find(|n| n.id == id) {
            n.read = true;
        }
    } else {
        // Mark all as read
        for n in &mut state.notifications {
            n.read = true;
        }
    }
    state_tx.send_replace(state_to_snapshot(&state));
    Ok(Value::Null)
}

// ---------------------------------------------------------------------------
// Payload helpers
// ---------------------------------------------------------------------------

fn extract_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

fn extract_u64(v: &Value, key: &str) -> Option<u64> {
    let val = v.get(key)?;
    val.as_u64().or_else(|| val.as_i64().map(|i| i as u64))
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum NotifError {
    #[error("zbus error: {0}")]
    Zbus(#[from] zbus::Error),
}
