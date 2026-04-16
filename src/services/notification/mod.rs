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
use crate::util::{json_map, unix_now_ms};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Spawn the `notification` service and return its [`ServiceHandle`].
pub fn spawn(_cfg: &Config) -> ServiceHandle {
    let initial = empty_snapshot();
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);
    let (events_tx, _) = broadcast::channel(64);

    tokio::spawn(run(request_rx, state_tx.clone(), events_tx.clone()));

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
) {
    info!("notification service started");

    let shared = Arc::new(RwLock::new(NotifState::default()));

    // Start the D-Bus server
    let dbus_result = start_dbus_server(shared.clone(), state_tx.clone(), events_tx.clone()).await;

    if let Err(e) = dbus_result {
        warn!(error = %e, "notification D-Bus server failed to start");
    }

    // Handle IPC requests
    while let Some(req) = request_rx.recv().await {
        handle_request(req, &shared, &state_tx, &events_tx).await;
    }

    info!("notification service stopped");
}

// ---------------------------------------------------------------------------
// D-Bus server
// ---------------------------------------------------------------------------

struct NotifServer {
    shared: Arc<RwLock<NotifState>>,
    state_tx: watch::Sender<Value>,
    events_tx: broadcast::Sender<Value>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
#[allow(clippy::too_many_arguments)]
impl NotifServer {
    async fn get_capabilities(&self) -> Vec<String> {
        vec![
            "body".to_string(),
            "actions".to_string(),
            "persistence".to_string(),
        ]
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
    ) -> u32 {
        let urgency = parse_urgency(&hints);
        let parsed_actions = parse_actions(&actions);
        let (app_name, summary) = normalize_notification_text(app_name, summary);

        let mut state = self.shared.write().await;
        let id = if replaces_id > 0 {
            let data = NotifData {
                app_name: &app_name,
                summary: &summary,
                body,
                icon: app_icon,
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
                icon: app_icon,
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
            "qsovd".to_string(),
            "quicksov".to_string(),
            "0.1.0".to_string(),
            "1.2".to_string(),
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
) -> Result<(), NotifError> {
    let server = NotifServer {
        shared,
        state_tx,
        events_tx,
    };

    let conn = zbus::connection::Builder::session()?
        .name("org.freedesktop.Notifications")?
        .serve_at("/org/freedesktop/Notifications", server)?
        .build()
        .await?;

    // Keep the connection alive in a background task
    tokio::spawn(async move {
        let _conn = conn;
        // Connection stays alive as long as this task runs
        futures::future::pending::<()>().await;
    });

    Ok(())
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

fn normalize_notification_text(app_name: &str, summary: &str) -> (String, String) {
    let app_name = app_name.trim();
    let summary = summary.trim();

    if !app_name.is_empty() {
        return (app_name.to_string(), summary.to_string());
    }

    if !summary.is_empty() {
        return (summary.to_string(), String::new());
    }

    (String::new(), String::new())
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
) {
    let result = match req.action.as_str() {
        "invoke_action" => handle_invoke_action(&req.payload, shared).await,
        "dismiss" => handle_dismiss(&req.payload, shared, state_tx, events_tx).await,
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
) -> Result<Value, ServiceError> {
    let id = extract_u64(payload, "id").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'id' field".to_string(),
    })? as u32;
    let _action_id =
        extract_str(payload, "action_id").ok_or_else(|| ServiceError::ActionPayload {
            msg: "missing 'action_id' field".to_string(),
        })?;
    let state = shared.read().await;
    if !state.notifications.iter().any(|n| n.id == id) {
        return Err(ServiceError::ActionPayload {
            msg: format!("notification {id} not found"),
        });
    }
    Ok(Value::Null)
}

async fn handle_dismiss(
    payload: &Value,
    shared: &Arc<RwLock<NotifState>>,
    state_tx: &watch::Sender<Value>,
    events_tx: &broadcast::Sender<Value>,
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
