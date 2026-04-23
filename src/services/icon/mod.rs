// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `icon` service — direct application icon / metadata lookup.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::info;

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::services::applications::{AppLookup, AppResolver};
use crate::util::json_map;

pub fn spawn(_cfg: &Config, apps: Arc<AppResolver>) -> ServiceHandle {
    let initial = snapshot(&apps);
    let (_state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, apps));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

fn snapshot(apps: &AppResolver) -> Value {
    json_map([
        ("availability", Value::from("ready")),
        (
            "desktop_entries",
            Value::from(apps.desktop_entry_count() as i64),
        ),
        ("icon_entries", Value::from(apps.icon_entry_count() as i64)),
    ])
}

async fn run(mut request_rx: mpsc::Receiver<ServiceRequest>, apps: Arc<AppResolver>) {
    info!("icon service started");

    while let Some(req) = request_rx.recv().await {
        let result = match req.action.as_str() {
            "resolve" => handle_resolve(&req.payload, &apps).await,
            other => Err(ServiceError::ActionUnknown {
                action: other.to_string(),
            }),
        };
        req.reply.send(result).ok();
    }

    info!("icon service stopped");
}

async fn handle_resolve(payload: &Value, apps: &AppResolver) -> Result<Value, ServiceError> {
    let lookup = AppLookup {
        icon_hint: optional_string(payload, "icon_hint"),
        desktop_entry: optional_string(payload, "desktop_entry"),
        app_id: optional_string(payload, "app_id"),
        wm_class: optional_string(payload, "wm_class"),
        app_name: optional_string(payload, "app_name"),
        binary: optional_string(payload, "binary"),
        process_id: optional_u32(payload, "process_id"),
    };

    if lookup.is_empty() {
        return Err(ServiceError::ActionPayload {
            msg: "expected at least one lookup field".to_string(),
        });
    }

    let resolved = apps.resolve(&lookup);
    Ok(json_map([
        ("display_name", Value::from(resolved.display_name.as_str())),
        ("icon", Value::from(resolved.icon.as_str())),
        ("icon_name", Value::from(resolved.icon_name.as_str())),
        (
            "desktop_entry",
            Value::from(resolved.desktop_entry.as_str()),
        ),
        ("match_source", Value::from(resolved.match_source.as_str())),
    ]))
}

fn optional_string(payload: &Value, key: &str) -> Option<String> {
    let value = payload.as_object()?.get(key)?.as_str()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn optional_u32(payload: &Value, key: &str) -> Option<u32> {
    let value = payload.as_object()?.get(key)?;
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .or_else(|| value.as_i64().and_then(|value| u32::try_from(value).ok()))
}
