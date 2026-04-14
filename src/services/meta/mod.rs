// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `meta` service — daemon self-description and health snapshot.
//!
//! This service has no external dependencies; all data comes from the daemon
//! process itself. It handles two actions:
//!
//! - `ping` — liveness check, returns `{ pong: true, server_time: <unix_secs> }`
//! - `shutdown` — reserved; currently returns `E_ACTION_UNKNOWN`

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::bus::ServiceError;
use crate::bus::{ServiceHandle, ServiceRequest};
use crate::util::is_empty_object;

/// Daemon binary version sourced from `Cargo.toml` at compile time.
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Spawn the `meta` service task and return its [`ServiceHandle`].
///
/// `started_at` is the process-level start instant used to compute `uptime_sec`.
/// `enabled_services` is the ordered list of all topics registered at startup;
/// it populates the `services` map in the snapshot.
pub fn spawn(
    started_at: Instant,
    enabled_services: Vec<String>,
    screens_roles: std::collections::HashMap<String, String>,
) -> ServiceHandle {
    let initial_snapshot = build_snapshot(started_at, &enabled_services, &screens_roles);
    let (state_tx, state_rx) = watch::channel(initial_snapshot);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

// ---------------------------------------------------------------------------
// Service task
// ---------------------------------------------------------------------------

async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    // Held to keep the watch channel open for subscribers; never written after init.
    _state_tx: watch::Sender<Value>,
) {
    info!("meta service started");

    while let Some(req) = request_rx.recv().await {
        dispatch_request(req);
    }

    info!("meta service stopped");
}

fn dispatch_request(req: ServiceRequest) {
    match req.action.as_str() {
        "ping" => handle_ping(req),
        "shutdown" => {
            // Reserved for future implementation.
            req.reply
                .send(Err(ServiceError::ActionUnknown {
                    action: req.action.clone(),
                }))
                .ok();
        }
        _ => {
            req.reply
                .send(Err(ServiceError::ActionUnknown {
                    action: req.action.clone(),
                }))
                .ok();
        }
    }
}

fn handle_ping(req: ServiceRequest) {
    if !is_empty_object(&req.payload) {
        warn!(
            payload = ?req.payload,
            "meta.ping rejected: payload must be an empty object"
        );
        req.reply
            .send(Err(ServiceError::ActionPayload {
                msg: "ping expects an empty object payload".to_string(),
            }))
            .ok();
        return;
    }

    let server_time = unix_now_secs();
    debug!(server_time, "meta.ping request handled");

    let payload = serde_json::json!({"pong": true, "server_time": server_time});

    req.reply.send(Ok(payload)).ok();
}

// ---------------------------------------------------------------------------
// Snapshot builder
// ---------------------------------------------------------------------------

/// Build a complete `meta` state snapshot as a `serde_json::Value` map.
fn build_snapshot(
    started_at: Instant,
    enabled_services: &[String],
    screens_roles: &std::collections::HashMap<String, String>,
) -> Value {
    let uptime_sec = started_at.elapsed().as_secs();

    let services_obj: serde_json::Map<String, Value> = enabled_services
        .iter()
        .map(|name| {
            let entry = serde_json::json!({"status": "healthy", "last_error": null});
            (name.clone(), entry)
        })
        .collect();

    let roles_obj: serde_json::Map<String, Value> = screens_roles
        .iter()
        .map(|(name, role)| (name.clone(), Value::from(role.as_str())))
        .collect();

    serde_json::json!({
        "server_version": SERVER_VERSION,
        "uptime_sec": uptime_sec,
        "services": Value::Object(services_obj),
        "config_needs_restart": false,
        "screens": {"roles": Value::Object(roles_obj)},
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Current Unix timestamp in whole seconds.
fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
