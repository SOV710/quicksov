// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceRequest};

use super::runtime::WifiRuntime;

pub(super) async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    state_tx: watch::Sender<Value>,
    ctrl_path: String,
    iface: String,
) {
    info!(ctrl = %ctrl_path, "net.wifi service started");

    let mut runtime = WifiRuntime::new(ctrl_path, iface);
    let mut last_snapshot = runtime.refresh_snapshot(true).await;
    state_tx.send_replace(last_snapshot.clone());

    let mut buf = vec![0u8; 4096];
    let mut poll_interval = tokio::time::interval(Duration::from_secs(3));
    poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        if !runtime.has_event_socket() && runtime.has_command_socket() {
            if let Err(error) = runtime.try_attach_event_socket().await {
                warn!(error = %error, "failed to attach wpa_supplicant event socket");
            }
        }

        if let Some(event_sock) = runtime.take_event_socket() {
            tokio::select! {
                req = request_rx.recv() => {
                    let Some(req) = req else { break };
                    handle_request(req, &mut runtime, &state_tx, &mut last_snapshot).await;
                    if runtime.has_command_socket() {
                        runtime.restore_event_socket(event_sock);
                    }
                }
                result = event_sock.recv(&mut buf) => {
                    match result {
                        Ok(n) => {
                            let msg = String::from_utf8_lossy(&buf[..n]);
                            if runtime.observe_wpa_event(&msg) {
                                publish_snapshot(&mut runtime, &state_tx, &mut last_snapshot, true)
                                    .await;
                            }
                            if runtime.has_command_socket() {
                                runtime.restore_event_socket(event_sock);
                            }
                        }
                        Err(error) => {
                            warn!(error = %error, "wpa_supplicant event socket recv error; falling back to polling");
                        }
                    }
                }
                _ = poll_interval.tick() => {
                    publish_snapshot(&mut runtime, &state_tx, &mut last_snapshot, true).await;
                    if runtime.has_command_socket() {
                        runtime.restore_event_socket(event_sock);
                    }
                }
            }
        } else {
            tokio::select! {
                req = request_rx.recv() => {
                    let Some(req) = req else { break };
                    handle_request(req, &mut runtime, &state_tx, &mut last_snapshot).await;
                }
                _ = poll_interval.tick() => {
                    publish_snapshot(&mut runtime, &state_tx, &mut last_snapshot, true).await;
                }
            }
        }
    }

    info!("net.wifi service stopped");
}

async fn handle_request(
    req: ServiceRequest,
    runtime: &mut WifiRuntime,
    state_tx: &watch::Sender<Value>,
    last_snapshot: &mut Value,
) {
    let action = req.action.clone();
    let result = match action.as_str() {
        "scan" | "scan_start" => runtime.handle_scan_start().await,
        "scan_stop" => runtime.handle_scan_stop().await,
        "connect" => runtime.handle_connect(&req.payload).await,
        "disconnect" => runtime.handle_disconnect().await,
        "forget" => runtime.handle_forget(&req.payload).await,
        "set_enabled" => runtime.handle_set_enabled(&req.payload).await,
        "set_airplane_mode" => runtime.handle_set_airplane_mode(&req.payload).await,
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };

    let allow_scan_promotion = !matches!(action.as_str(), "scan" | "scan_start")
        || result.is_err()
        || runtime.scan_state() != super::model::WifiScanState::Starting;
    publish_snapshot(runtime, state_tx, last_snapshot, allow_scan_promotion).await;
    req.reply.send(result).ok();
}

async fn publish_snapshot(
    runtime: &mut WifiRuntime,
    state_tx: &watch::Sender<Value>,
    last_snapshot: &mut Value,
    allow_scan_promotion: bool,
) {
    let snapshot = runtime.refresh_snapshot(allow_scan_promotion).await;
    if snapshot != *last_snapshot {
        *last_snapshot = snapshot.clone();
        state_tx.send_replace(snapshot);
    }
}
