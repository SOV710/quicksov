// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `theme` service — design tokens loader.
//!
//! Reads `~/.config/quicksov/design-tokens.toml` when available, falling back
//! to the compile-time-embedded `config/theme_tokyonight.json`.

use rmpv::Value;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::{json_to_rmpv, toml_to_rmpv};

/// Embedded fallback theme (Tokyo Night).
const EMBEDDED_THEME_JSON: &str = include_str!("../../../config/theme_tokyonight.json");

/// Spawn the `theme` service and return its [`ServiceHandle`].
pub fn spawn(_cfg: &Config) -> ServiceHandle {
    let snapshot = load_theme();
    let (state_tx, state_rx) = watch::channel(snapshot);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

fn load_theme() -> Value {
    // Try user config file first
    if let Some(home) = dirs::home_dir() {
        let path = home
            .join(".config")
            .join("quicksov")
            .join("design-tokens.toml");
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(text) => match text.parse::<toml::Value>() {
                    Ok(table) => {
                        debug!(path = %path.display(), "loaded design-tokens.toml");
                        return toml_to_rmpv(&table);
                    }
                    Err(e) => warn!(error = %e, "failed to parse design-tokens.toml"),
                },
                Err(e) => warn!(error = %e, "failed to read design-tokens.toml"),
            }
        }
    }

    // Fallback to embedded JSON
    debug!("using embedded Tokyo Night theme");
    match serde_json::from_str::<serde_json::Value>(EMBEDDED_THEME_JSON) {
        Ok(v) => json_to_rmpv(&v),
        Err(e) => {
            warn!(error = %e, "failed to parse embedded theme JSON");
            Value::Nil
        }
    }
}

async fn run(mut request_rx: mpsc::Receiver<ServiceRequest>, _state_tx: watch::Sender<Value>) {
    info!("theme service started");
    while let Some(req) = request_rx.recv().await {
        // Theme has no actions
        req.reply
            .send(Err(ServiceError::ActionUnknown {
                action: req.action.clone(),
            }))
            .ok();
    }
    info!("theme service stopped");
}
