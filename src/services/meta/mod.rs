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

use rmpv::Value;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::bus::ServiceError;
use crate::bus::{ServiceHandle, ServiceRequest};

/// Daemon binary version sourced from `Cargo.toml` at compile time.
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Spawn the `meta` service task and return its [`ServiceHandle`].
///
/// `started_at` is the process-level start instant used to compute `uptime_sec`.
/// `enabled_services` is the ordered list of all topics registered at startup;
/// it populates the `services` map in the snapshot.
pub fn spawn(started_at: Instant, enabled_services: Vec<String>) -> ServiceHandle {
    let initial_snapshot = build_snapshot(started_at, &enabled_services);
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

    let payload = Value::Map(vec![
        (Value::from("pong"), Value::Boolean(true)),
        (Value::from("server_time"), Value::from(server_time)),
    ]);

    req.reply.send(Ok(payload)).ok();
}

// ---------------------------------------------------------------------------
// Snapshot builder
// ---------------------------------------------------------------------------

/// Build a complete `meta` state snapshot as an `rmpv::Value` map.
fn build_snapshot(started_at: Instant, enabled_services: &[String]) -> Value {
    let uptime_sec = started_at.elapsed().as_secs();

    let services_map: Vec<(Value, Value)> = enabled_services
        .iter()
        .map(|name| {
            let entry = Value::Map(vec![
                (Value::from("status"), Value::from("healthy")),
                (Value::from("last_error"), Value::Nil),
            ]);
            (Value::from(name.as_str()), entry)
        })
        .collect();

    Value::Map(vec![
        (Value::from("server_version"), Value::from(SERVER_VERSION)),
        (Value::from("uptime_sec"), Value::from(uptime_sec)),
        (Value::from("services"), Value::Map(services_map)),
        (Value::from("config_needs_restart"), Value::Boolean(false)),
    ])
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `v` represents an empty object (Nil or an empty map).
///
/// Clients that omit the payload field entirely send `Nil`; clients that
/// explicitly send `{}` send an empty msgpack map. Both are valid for `ping`.
fn is_empty_object(v: &Value) -> bool {
    match v {
        Value::Nil => true,
        Value::Map(m) => m.is_empty(),
        _ => false,
    }
}

/// Current Unix timestamp in whole seconds.
fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
