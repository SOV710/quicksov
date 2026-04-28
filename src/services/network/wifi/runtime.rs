// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::net::UnixDatagram;
use tracing::warn;

use crate::bus::ServiceError;

use super::client::{WpaCtrlClient, WpaNetworkSecurity};
use super::command::{enabled_from_payload, escape_wpa_string, ConnectRequest, ForgetRequest};
use super::error::{service_error_from_wifi_error, WifiError};
use super::manual::{ManualConnectOutcome, ManualConnectTracker};
#[cfg(test)]
use super::manual::{ManualConnectReason, ManualConnectState};
use super::model::{ScanRequestOutcome, WifiReadState, WifiScanState};
use super::probe::{availability_reason_from_wifi_error, classify_status, inspect_environment};
use super::scan::ScanTracker;
use super::snapshot::build_wifi_snapshot;

pub(super) struct WifiRuntime {
    iface: String,
    client: WpaCtrlClient,
    pub(super) scan: ScanTracker,
    manual_connect: ManualConnectTracker,
}

impl WifiRuntime {
    pub(super) fn new(ctrl_path: String, iface: String) -> Self {
        Self {
            iface,
            client: WpaCtrlClient::new(ctrl_path),
            scan: ScanTracker::default(),
            manual_connect: ManualConnectTracker::default(),
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

    #[cfg(test)]
    pub(super) fn observe_wpa_event(&mut self, message: &str) -> bool {
        self.scan.observe_wpa_event(message)
    }

    pub(super) async fn handle_wpa_event(&mut self, message: &str) -> bool {
        let scan_relevant = self.scan.observe_wpa_event(message);
        let manual_outcome = self.manual_connect.observe_wpa_event(message);
        let manual_relevant = manual_outcome.changed();
        self.apply_manual_connect_outcome(manual_outcome).await;
        scan_relevant || manual_relevant
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

    #[cfg(test)]
    pub(super) fn set_command_socket_for_test(&mut self, sock: UnixDatagram) {
        self.client.set_command_socket_for_test(sock);
    }

    #[cfg(test)]
    pub(super) fn start_manual_connect_for_test(
        &mut self,
        target_id: &str,
        target_ssid: &str,
        started_at: i64,
        restore_enabled_ids: Vec<String>,
    ) {
        self.manual_connect.start(
            target_id.to_string(),
            target_ssid.to_string(),
            started_at,
            restore_enabled_ids,
        );
    }

    #[cfg(test)]
    pub(super) fn manual_connect_state(&self) -> ManualConnectState {
        self.manual_connect.state()
    }

    #[cfg(test)]
    pub(super) fn manual_connect_reason(&self) -> ManualConnectReason {
        self.manual_connect.reason()
    }

    #[cfg(test)]
    pub(super) fn manual_connect_target_id(&self) -> Option<&str> {
        self.manual_connect.target_id()
    }

    #[cfg(test)]
    pub(super) async fn reconcile_manual_connect_for_test(
        &mut self,
        state: &WifiReadState,
        now_ms: i64,
    ) -> bool {
        self.reconcile_manual_connect(state, now_ms).await
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
                    self.reconcile_manual_connect(&state, unix_ms_now()).await;
                    wifi_state = state;
                    backend_ready = true;
                }
                Err(err) => {
                    ctrl_problem = Some(availability_reason_from_wifi_error(&err));
                    self.scan.clear_activity();
                    let outcome = self.manual_connect.observe_backend_error();
                    self.apply_manual_connect_outcome(outcome).await;
                    self.client.drop_sockets();
                }
            }
        } else {
            self.scan.clear_activity();
            let outcome = self.manual_connect.observe_backend_error();
            self.apply_manual_connect_outcome(outcome).await;
            self.client.drop_sockets();
        }

        let status = classify_status(&probe, backend_ready, ctrl_problem);
        build_wifi_snapshot(
            &self.iface,
            &status,
            &wifi_state,
            &self.scan,
            &self.manual_connect,
        )
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
                let security = self.network_security_for_request(&request.ssid, true).await;
                self.configure_network_security(&existing.id, &request.ssid, security, Some(key))
                    .await?;
            }
            existing.id.clone()
        } else {
            let security = self
                .network_security_for_request(&request.ssid, request.psk.is_some())
                .await;
            if security == WpaNetworkSecurity::Unsupported {
                return Err(ServiceError::ActionPayload {
                    msg: format!("network '{}' uses unsupported security", request.ssid),
                });
            }
            if security.requires_passphrase() && request.psk.is_none() {
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

            self.configure_network_security(&id, &request.ssid, security, escaped_psk.as_deref())
                .await?;

            id
        };

        let restore_enabled_ids = saved_networks
            .iter()
            .filter(|network| network.id != id && !network.flags.contains("[DISABLED]"))
            .map(|network| network.id.clone())
            .collect::<Vec<_>>();

        if request.save {
            self.client
                .enable_network_no_connect(&id)
                .await
                .map_err(service_error_from_wifi_error)?;
            self.client
                .save_config()
                .await
                .map_err(service_error_from_wifi_error)?;
        }

        if let Err(err) = self.client.select_network(&id).await {
            self.manual_connect
                .start(id, request.ssid, unix_ms_now(), restore_enabled_ids);
            let outcome = self.manual_connect.observe_backend_error();
            self.apply_manual_connect_outcome(outcome).await;
            return Err(service_error_from_wifi_error(err));
        }

        self.manual_connect
            .start(id, request.ssid, unix_ms_now(), restore_enabled_ids);

        Ok(Value::Null)
    }

    async fn network_security_for_request(&self, ssid: &str, has_psk: bool) -> WpaNetworkSecurity {
        self.client
            .scan_result_security(ssid)
            .await
            .unwrap_or(if has_psk {
                WpaNetworkSecurity::Psk
            } else {
                WpaNetworkSecurity::Open
            })
    }

    async fn configure_network_security(
        &self,
        id: &str,
        ssid: &str,
        security: WpaNetworkSecurity,
        escaped_psk: Option<&str>,
    ) -> Result<(), ServiceError> {
        match (security, escaped_psk) {
            (WpaNetworkSecurity::Unsupported, _) => Err(ServiceError::ActionPayload {
                msg: format!("network '{ssid}' uses unsupported security"),
            }),
            (WpaNetworkSecurity::Open, None) => self
                .client
                .set_network_open(id)
                .await
                .map_err(service_error_from_wifi_error),
            (WpaNetworkSecurity::Open | WpaNetworkSecurity::Psk, Some(key)) => {
                self.client
                    .set_network_key_mgmt(id, "WPA-PSK")
                    .await
                    .map_err(service_error_from_wifi_error)?;
                self.client
                    .set_network_psk(id, key)
                    .await
                    .map_err(service_error_from_wifi_error)
            }
            (WpaNetworkSecurity::Psk, None)
            | (WpaNetworkSecurity::Sae, None)
            | (WpaNetworkSecurity::SaeTransition, None) => Err(ServiceError::ActionPayload {
                msg: format!("network '{ssid}' requires a psk"),
            }),
            (WpaNetworkSecurity::Sae | WpaNetworkSecurity::SaeTransition, Some(key)) => {
                self.client
                    .set_network_key_mgmt(id, "SAE")
                    .await
                    .map_err(service_error_from_wifi_error)?;
                self.client
                    .set_network_sae_password(id, key)
                    .await
                    .map_err(service_error_from_wifi_error)?;
                self.client
                    .set_network_ieee80211w(id, 2)
                    .await
                    .map_err(service_error_from_wifi_error)
            }
        }
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
        let outcome = self.manual_connect.observe_backend_error();
        self.apply_manual_connect_outcome(outcome).await;
        self.client.drop_sockets();
        Ok(Value::Null)
    }

    pub(super) async fn handle_set_airplane_mode(
        &mut self,
        payload: &Value,
    ) -> Result<Value, ServiceError> {
        let enabled = enabled_from_payload(payload)?;
        run_rfkill(["unblock", "all"], ["block", "all"], !enabled).await?;
        let outcome = self.manual_connect.observe_backend_error();
        self.apply_manual_connect_outcome(outcome).await;
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

    async fn reconcile_manual_connect(&mut self, state: &WifiReadState, now_ms: i64) -> bool {
        let outcome = self.manual_connect.observe_read_state(state, now_ms);
        let changed = outcome.changed();
        self.apply_manual_connect_outcome(outcome).await;
        changed
    }

    async fn apply_manual_connect_outcome(&mut self, outcome: ManualConnectOutcome) {
        match outcome {
            ManualConnectOutcome::None => {}
            ManualConnectOutcome::Succeeded {
                restore_enabled_ids,
            }
            | ManualConnectOutcome::Failed {
                restore_enabled_ids,
            } => {
                self.restore_manual_enabled_networks(restore_enabled_ids)
                    .await
            }
        }
    }

    async fn restore_manual_enabled_networks(&mut self, restore_enabled_ids: Vec<String>) {
        for id in restore_enabled_ids {
            if let Err(error) = self.client.enable_network_no_connect(&id).await {
                warn!(
                    network_id = %id,
                    error = %error,
                    "failed to restore enabled Wi-Fi network after manual connect"
                );
                self.client.drop_sockets();
                break;
            }
        }
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

fn unix_ms_now() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
