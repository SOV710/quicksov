// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::{Command, Output};

use serde_json::Value;
use tokio::net::UnixDatagram;

use crate::bus::ServiceError;

use super::client::WpaCtrlClient;
use super::command::{enabled_from_payload, escape_wpa_string, ConnectRequest, ForgetRequest};
use super::error::{service_error_from_wifi_error, WifiError};
use super::model::{ScanRequestOutcome, WifiReadState, WifiScanState};
use super::probe::{availability_reason_from_wifi_error, classify_status, inspect_environment};
use super::scan::ScanTracker;
use super::snapshot::build_wifi_snapshot;

pub(super) struct WifiRuntime {
    iface: String,
    client: WpaCtrlClient,
    pub(super) scan: ScanTracker,
}

impl WifiRuntime {
    pub(super) fn new(ctrl_path: String, iface: String) -> Self {
        Self {
            iface,
            client: WpaCtrlClient::new(ctrl_path),
            scan: ScanTracker::default(),
        }
    }

    pub(super) fn has_command_socket(&self) -> bool {
        self.client.has_command_socket()
    }

    pub(super) fn has_event_socket(&self) -> bool {
        self.client.has_event_socket()
    }

    pub(super) fn take_event_socket(&mut self) -> Option<UnixDatagram> {
        self.client.take_event_socket()
    }

    pub(super) fn restore_event_socket(&mut self, sock: UnixDatagram) {
        self.client.restore_event_socket(sock);
    }

    pub(super) async fn try_attach_event_socket(&mut self) -> Result<(), WifiError> {
        self.client.try_attach_event_socket().await
    }

    pub(super) fn scan_state(&self) -> WifiScanState {
        self.scan.state()
    }

    #[cfg(test)]
    pub(super) fn scan_started_at(&self) -> Option<i64> {
        self.scan.started_at()
    }

    #[cfg(test)]
    pub(super) fn scan_finished_at(&self) -> Option<i64> {
        self.scan.finished_at()
    }

    #[cfg(test)]
    pub(super) fn scan_last_error(&self) -> Option<&str> {
        self.scan.last_error()
    }

    #[cfg(test)]
    pub(super) fn scan_stop_requested(&self) -> bool {
        self.scan.stop_requested()
    }

    #[cfg(test)]
    pub(super) fn observe_status_scan(&mut self, status_scan_active: bool) {
        self.scan.observe_status_scan(status_scan_active);
    }

    pub(super) fn observe_wpa_event(&mut self, message: &str) -> bool {
        self.scan.observe_wpa_event(message)
    }

    pub(super) fn finish_scan_start(
        &mut self,
        outcome: Result<ScanRequestOutcome, WifiError>,
    ) -> Result<Value, ServiceError> {
        self.scan
            .finish_scan_start(outcome)
            .map_err(service_error_from_wifi_error)?;
        Ok(Value::Null)
    }

    pub(super) fn finish_scan_stop(
        &mut self,
        result: Result<(), WifiError>,
    ) -> Result<Value, ServiceError> {
        self.scan
            .finish_scan_stop(result)
            .map_err(service_error_from_wifi_error)?;
        Ok(Value::Null)
    }

    #[cfg(test)]
    pub(super) fn mark_scan_runtime_failed(&mut self, error: String) {
        self.scan.mark_runtime_failed(error);
    }

    pub(super) async fn refresh_snapshot(&mut self, allow_scan_promotion: bool) -> Value {
        let probe = inspect_environment(&self.iface, self.client.ctrl_path());
        let mut wifi_state = WifiReadState::default();
        let mut ctrl_problem = None;
        let mut backend_ready = false;

        if probe.present {
            match self.client.read_state().await {
                Ok(state) => {
                    if allow_scan_promotion {
                        self.scan.observe_status_scan(state.status_scan_active);
                    }
                    wifi_state = state;
                    backend_ready = true;
                }
                Err(err) => {
                    ctrl_problem = Some(availability_reason_from_wifi_error(&err));
                    self.scan.clear_activity();
                    self.client.drop_sockets();
                }
            }
        } else {
            self.scan.clear_activity();
            self.client.drop_sockets();
        }

        let status = classify_status(&probe, backend_ready, ctrl_problem);
        build_wifi_snapshot(&self.iface, &status, &wifi_state, &self.scan)
    }

    pub(super) async fn handle_scan_start(&mut self) -> Result<Value, ServiceError> {
        if self.scan.is_idle() {
            self.require_backend_ready().await?;
            let outcome = self.client.request_scan().await;
            return self.finish_scan_start(outcome);
        }

        Ok(Value::Null)
    }

    pub(super) async fn handle_scan_stop(&mut self) -> Result<Value, ServiceError> {
        if self.scan.is_idle() {
            return Ok(Value::Null);
        }

        self.require_backend_ready().await?;
        let result = self.client.abort_scan().await;
        self.finish_scan_stop(result)
    }

    pub(super) async fn handle_connect(&mut self, payload: &Value) -> Result<Value, ServiceError> {
        self.require_backend_ready().await?;

        let request = ConnectRequest::from_payload(payload)?;
        let escaped_ssid = escape_wpa_string(&request.ssid)?;
        let escaped_psk = request.psk.as_deref().map(escape_wpa_string).transpose()?;

        let saved_networks = self
            .client
            .list_saved_networks()
            .await
            .map_err(service_error_from_wifi_error)?;

        let id = if let Some(existing) = saved_networks
            .iter()
            .find(|network| network.ssid == request.ssid)
        {
            if let Some(key) = escaped_psk.as_deref() {
                self.client
                    .set_network_psk(&existing.id, key)
                    .await
                    .map_err(service_error_from_wifi_error)?;
            }
            existing.id.clone()
        } else {
            let secure = self.client.scan_result_requires_psk(&request.ssid).await;
            if secure && request.psk.is_none() {
                return Err(ServiceError::ActionPayload {
                    msg: format!("network '{}' requires a psk", request.ssid),
                });
            }

            let id = self
                .client
                .add_network()
                .await
                .map_err(service_error_from_wifi_error)?;
            self.client
                .set_network_ssid(&id, &escaped_ssid)
                .await
                .map_err(service_error_from_wifi_error)?;

            if let Some(key) = escaped_psk.as_deref() {
                self.client
                    .set_network_psk(&id, key)
                    .await
                    .map_err(service_error_from_wifi_error)?;
            } else {
                self.client
                    .set_network_open(&id)
                    .await
                    .map_err(service_error_from_wifi_error)?;
            }

            id
        };

        if request.save {
            self.client
                .enable_network(&id)
                .await
                .map_err(service_error_from_wifi_error)?;
        }

        self.client
            .select_network(&id)
            .await
            .map_err(service_error_from_wifi_error)?;

        if request.save {
            self.client
                .save_config()
                .await
                .map_err(service_error_from_wifi_error)?;
        }

        Ok(Value::Null)
    }

    pub(super) async fn handle_disconnect(&mut self) -> Result<Value, ServiceError> {
        self.require_backend_ready().await?;
        self.client
            .disconnect()
            .await
            .map_err(service_error_from_wifi_error)?;
        Ok(Value::Null)
    }

    pub(super) async fn handle_forget(&mut self, payload: &Value) -> Result<Value, ServiceError> {
        self.require_backend_ready().await?;
        let request = ForgetRequest::from_payload(payload)?;
        let saved_networks = self
            .client
            .list_saved_networks()
            .await
            .map_err(service_error_from_wifi_error)?;

        for network in saved_networks {
            if network.ssid == request.ssid {
                self.client
                    .remove_network(&network.id)
                    .await
                    .map_err(service_error_from_wifi_error)?;
                self.client
                    .save_config()
                    .await
                    .map_err(service_error_from_wifi_error)?;
                return Ok(Value::Null);
            }
        }

        Err(ServiceError::ActionPayload {
            msg: format!("network '{}' not found", request.ssid),
        })
    }

    pub(super) async fn handle_set_enabled(
        &mut self,
        payload: &Value,
    ) -> Result<Value, ServiceError> {
        let enabled = enabled_from_payload(payload)?;
        run_rfkill(["unblock", "wifi"], ["block", "wifi"], enabled).await?;
        self.client.drop_sockets();
        Ok(Value::Null)
    }

    pub(super) async fn handle_set_airplane_mode(
        &mut self,
        payload: &Value,
    ) -> Result<Value, ServiceError> {
        let enabled = enabled_from_payload(payload)?;
        run_rfkill(["unblock", "all"], ["block", "all"], !enabled).await?;
        self.client.drop_sockets();
        Ok(Value::Null)
    }

    async fn require_backend_ready(&mut self) -> Result<(), ServiceError> {
        let probe = inspect_environment(&self.iface, self.client.ctrl_path());

        if !probe.present {
            return Err(ServiceError::Internal {
                msg: "wifi unavailable: no_adapter".to_string(),
            });
        }

        if probe.rfkill_hard_blocked {
            return Err(ServiceError::Internal {
                msg: "wifi disabled: rfkill_hard_blocked".to_string(),
            });
        }

        if probe.rfkill_soft_blocked {
            return Err(ServiceError::Internal {
                msg: "wifi disabled: rfkill_soft_blocked".to_string(),
            });
        }

        self.client
            .ensure_command_socket()
            .await
            .map_err(service_error_from_wifi_error)?;

        Ok(())
    }
}

async fn run_rfkill(
    unblock_args: [&str; 2],
    block_args: [&str; 2],
    enabled: bool,
) -> Result<(), ServiceError> {
    let args = if enabled { unblock_args } else { block_args };
    let owned_args = args.map(str::to_string).to_vec();

    tokio::task::spawn_blocking(move || run_rfkill_blocking(&owned_args))
        .await
        .map_err(|err| ServiceError::Internal {
            msg: format!("rfkill task failed: {err}"),
        })?
}

fn run_rfkill_blocking(args: &[String]) -> Result<(), ServiceError> {
    let output =
        Command::new("rfkill")
            .args(args)
            .output()
            .map_err(|err| ServiceError::Internal {
                msg: format!("failed to run rfkill: {err}"),
            })?;

    if output.status.success() {
        return Ok(());
    }

    Err(ServiceError::Internal {
        msg: format!(
            "rfkill {} failed: {}",
            args.join(" "),
            command_error_text(&output)
        ),
    })
}

fn command_error_text(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        output.status.to_string()
    } else {
        stderr
    }
}
