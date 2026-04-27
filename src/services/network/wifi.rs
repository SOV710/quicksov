// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `net.wifi` service — wpa_supplicant control socket backend.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::net::UnixDatagram;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::json_map;

const DEFAULT_WIFI_BACKEND: &str = "wpa_supplicant";
const DEFAULT_WPA_CTRL_DIR: &str = "/run/wpa_supplicant";
const DEFAULT_WIFI_INTERFACE: &str = "wlo1";

/// Spawn the `net.wifi` service and return its [`ServiceHandle`].
pub fn spawn_wifi(cfg: &Config) -> ServiceHandle {
    let (ctrl_path, iface) = resolve_wifi_control(cfg);

    let initial = unavailable_snapshot(&iface);
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx, ctrl_path, iface));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

fn unavailable_snapshot(iface: &str) -> Value {
    build_wifi_snapshot(
        iface,
        &WifiStatus::default(),
        &WifiReadState::default(),
        WifiScanState::Idle,
        None,
        None,
        None,
    )
}

fn resolve_wifi_control(cfg: &Config) -> (String, String) {
    let network = cfg.services.network.as_ref();

    if let Some(backend) = network.and_then(|entry| entry.wifi_backend.as_deref()) {
        if backend != DEFAULT_WIFI_BACKEND {
            warn!(
                backend = %backend,
                fallback = DEFAULT_WIFI_BACKEND,
                "unsupported wifi backend configured; falling back to wpa_supplicant"
            );
        }
    }

    if let Some(path) = network
        .and_then(|entry| entry.wpa_ctrl_path.as_ref())
        .map(ToString::to_string)
    {
        let iface =
            iface_from_ctrl_path(&path).unwrap_or_else(|| DEFAULT_WIFI_INTERFACE.to_string());
        return (path, iface);
    }

    let configured_ifaces = network
        .and_then(|entry| entry.interfaces.as_ref())
        .map(|interfaces| {
            interfaces
                .iter()
                .map(|iface| iface.trim())
                .filter(|iface| !iface.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let iface_candidates = if configured_ifaces.is_empty() {
        discover_wpa_control_interfaces()
    } else {
        configured_ifaces
    };

    for iface in &iface_candidates {
        let ctrl_path = default_wpa_ctrl_path(iface);
        if Path::new(&ctrl_path).exists() {
            return (ctrl_path, iface.clone());
        }
    }

    if let Some(iface) = iface_candidates.first() {
        return (default_wpa_ctrl_path(iface), iface.clone());
    }

    let iface = DEFAULT_WIFI_INTERFACE.to_string();
    (default_wpa_ctrl_path(&iface), iface)
}

fn iface_from_ctrl_path(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
}

fn default_wpa_ctrl_path(iface: &str) -> String {
    format!("{DEFAULT_WPA_CTRL_DIR}/{iface}")
}

fn discover_wpa_control_interfaces() -> Vec<String> {
    discover_wpa_control_interfaces_in(Path::new(DEFAULT_WPA_CTRL_DIR))
}

fn discover_wpa_control_interfaces_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut interfaces = entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_socket() {
                return None;
            }
            entry.file_name().into_string().ok()
        })
        .collect::<Vec<_>>();
    interfaces.sort();
    interfaces
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn run(
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
        if runtime.event_sock.is_none() && runtime.cmd_sock.is_some() {
            runtime.try_attach_event_socket().await;
        }

        if let Some(event_sock) = runtime.event_sock.take() {
            tokio::select! {
                req = request_rx.recv() => {
                    let Some(req) = req else { break };
                    handle_request(req, &mut runtime, &state_tx, &mut last_snapshot).await;
                    if runtime.cmd_sock.is_some() {
                        runtime.event_sock = Some(event_sock);
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
                            if runtime.cmd_sock.is_some() {
                                runtime.event_sock = Some(event_sock);
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "wpa_supplicant event socket recv error; falling back to polling");
                        }
                    }
                }
                _ = poll_interval.tick() => {
                    publish_snapshot(&mut runtime, &state_tx, &mut last_snapshot, true).await;
                    if runtime.cmd_sock.is_some() {
                        runtime.event_sock = Some(event_sock);
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
        || runtime.scan_state != WifiScanState::Starting;
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

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

struct WifiRuntime {
    ctrl_path: String,
    iface: String,
    cmd_sock: Option<UnixDatagram>,
    event_sock: Option<UnixDatagram>,
    scan_state: WifiScanState,
    scan_started_at: Option<i64>,
    scan_finished_at: Option<i64>,
    scan_last_error: Option<String>,
    scan_stop_requested: bool,
}

impl WifiRuntime {
    fn new(ctrl_path: String, iface: String) -> Self {
        Self {
            ctrl_path,
            iface,
            cmd_sock: None,
            event_sock: None,
            scan_state: WifiScanState::Idle,
            scan_started_at: None,
            scan_finished_at: None,
            scan_last_error: None,
            scan_stop_requested: false,
        }
    }

    fn drop_sockets(&mut self) {
        self.cmd_sock = None;
        self.event_sock = None;
    }

    async fn ensure_command_socket(&mut self) -> Result<(), WifiError> {
        if self.cmd_sock.is_some() {
            return Ok(());
        }

        let sock = open_wpa_socket(&self.ctrl_path, "cmd").await?;
        self.cmd_sock = Some(sock);
        Ok(())
    }

    async fn try_attach_event_socket(&mut self) {
        let sock = match open_wpa_socket(&self.ctrl_path, "evt").await {
            Ok(sock) => sock,
            Err(e) => {
                warn!(error = %e, "failed to open wpa_supplicant event socket");
                return;
            }
        };

        match wpa_expect_ok(&sock, "ATTACH").await {
            Ok(()) => {
                self.event_sock = Some(sock);
            }
            Err(e) => {
                warn!(error = %e, "failed to attach wpa_supplicant event socket");
            }
        }
    }

    fn mark_scan_starting(&mut self) {
        self.scan_state = WifiScanState::Starting;
        self.scan_started_at = Some(unix_ms_now());
        self.scan_last_error = None;
        self.scan_stop_requested = false;
    }

    fn mark_scan_running(&mut self) {
        self.scan_state = WifiScanState::Running;
        if self.scan_started_at.is_none() {
            self.scan_started_at = Some(unix_ms_now());
        }
    }

    fn mark_scan_finished(&mut self) {
        self.scan_state = WifiScanState::Idle;
        self.scan_finished_at = Some(unix_ms_now());
        self.scan_stop_requested = false;
    }

    fn mark_scan_start_failed(&mut self, error: String) {
        self.scan_state = WifiScanState::Idle;
        self.scan_last_error = Some(error);
        self.scan_stop_requested = false;
    }

    fn mark_scan_runtime_failed(&mut self, error: String) {
        self.scan_state = WifiScanState::Idle;
        self.scan_finished_at = Some(unix_ms_now());
        self.scan_last_error = Some(error);
        self.scan_stop_requested = false;
    }

    fn mark_scan_stop_requested(&mut self) {
        self.scan_stop_requested = true;
    }

    fn clear_scan_activity(&mut self) {
        self.scan_state = WifiScanState::Idle;
        self.scan_stop_requested = false;
    }

    fn observe_status_scan(&mut self, status_scan_active: bool) {
        if status_scan_active {
            self.mark_scan_running();
        } else if self.scan_stop_requested && self.scan_state != WifiScanState::Idle {
            self.mark_scan_finished();
        }
    }

    fn observe_wpa_event(&mut self, message: &str) -> bool {
        let mut relevant = false;

        if message.contains("CTRL-EVENT-SCAN-STARTED") {
            self.mark_scan_running();
            relevant = true;
        }

        if message.contains("CTRL-EVENT-SCAN-RESULTS") {
            self.mark_scan_finished();
            relevant = true;
        }

        if message.contains("CTRL-EVENT-SCAN-FAILED") {
            if self.scan_stop_requested {
                self.mark_scan_finished();
            } else {
                self.mark_scan_runtime_failed(message.trim().to_string());
            }
            relevant = true;
        }

        relevant
            || message.contains("CTRL-EVENT-CONNECTED")
            || message.contains("CTRL-EVENT-DISCONNECTED")
            || message.contains("CTRL-EVENT-SSID-TEMP-DISABLED")
            || message.contains("CTRL-EVENT-SSID-REENABLED")
    }

    async fn refresh_snapshot(&mut self, allow_scan_promotion: bool) -> Value {
        let probe = inspect_environment(&self.iface, &self.ctrl_path);
        let mut wifi_state = WifiReadState::default();
        let mut ctrl_problem = None;
        let mut backend_ready = false;

        if probe.present {
            match self.read_wpa_state().await {
                Ok(state) => {
                    if allow_scan_promotion {
                        self.observe_status_scan(state.status_scan_active);
                    }
                    wifi_state = state;
                    backend_ready = true;
                }
                Err(err) => {
                    ctrl_problem = Some(availability_reason_from_wifi_error(&err));
                    self.clear_scan_activity();
                    self.drop_sockets();
                }
            }
        } else {
            self.clear_scan_activity();
            self.drop_sockets();
        }

        let status = classify_status(&probe, backend_ready, ctrl_problem);
        build_wifi_snapshot(
            &self.iface,
            &status,
            &wifi_state,
            self.scan_state,
            self.scan_started_at,
            self.scan_finished_at,
            self.scan_last_error.as_deref(),
        )
    }

    async fn read_wpa_state(&mut self) -> Result<WifiReadState, WifiError> {
        self.ensure_command_socket().await?;

        let sock = self.cmd_sock.as_ref().ok_or_else(|| WifiError::Io {
            context: "missing wpa_supplicant command socket after connect".to_string(),
            source: io::Error::new(io::ErrorKind::NotConnected, "command socket unavailable"),
        })?;

        read_full_state(sock, &self.iface).await
    }

    async fn require_backend_ready(&mut self) -> Result<(), ServiceError> {
        let probe = inspect_environment(&self.iface, &self.ctrl_path);

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

        self.ensure_command_socket()
            .await
            .map_err(service_error_from_wifi_error)?;

        Ok(())
    }

    async fn handle_scan_start(&mut self) -> Result<Value, ServiceError> {
        if self.scan_state != WifiScanState::Idle {
            return Ok(Value::Null);
        }

        self.require_backend_ready().await?;
        let outcome = {
            let sock = self
                .cmd_sock
                .as_ref()
                .ok_or_else(|| ServiceError::Internal {
                    msg: "wifi backend unavailable after socket setup".to_string(),
                })?;
            wpa_request_scan(sock).await
        };

        self.finish_scan_start(outcome)
    }

    fn finish_scan_start(
        &mut self,
        outcome: Result<ScanRequestOutcome, WifiError>,
    ) -> Result<Value, ServiceError> {
        match outcome {
            Ok(ScanRequestOutcome::Started) => self.mark_scan_starting(),
            Ok(ScanRequestOutcome::Busy) => {
                self.mark_scan_running();
                self.scan_stop_requested = false;
            }
            Err(err) => {
                let error_text = err.to_string();
                self.mark_scan_start_failed(error_text);
                return Err(service_error_from_wifi_error(err));
            }
        }

        Ok(Value::Null)
    }

    async fn handle_scan_stop(&mut self) -> Result<Value, ServiceError> {
        if self.scan_state == WifiScanState::Idle {
            return Ok(Value::Null);
        }

        self.require_backend_ready().await?;
        let result = {
            let sock = self
                .cmd_sock
                .as_ref()
                .ok_or_else(|| ServiceError::Internal {
                    msg: "wifi backend unavailable after socket setup".to_string(),
                })?;
            wpa_abort_scan(sock).await
        };

        self.finish_scan_stop(result)
    }

    fn finish_scan_stop(&mut self, result: Result<(), WifiError>) -> Result<Value, ServiceError> {
        match result {
            Ok(()) => {
                self.mark_scan_stop_requested();
                Ok(Value::Null)
            }
            Err(err) => Err(service_error_from_wifi_error(err)),
        }
    }

    async fn handle_connect(&mut self, payload: &Value) -> Result<Value, ServiceError> {
        self.require_backend_ready().await?;
        let sock = self
            .cmd_sock
            .as_ref()
            .ok_or_else(|| ServiceError::Internal {
                msg: "wifi backend unavailable after socket setup".to_string(),
            })?;

        let raw_ssid = extract_str(payload, "ssid").ok_or_else(|| ServiceError::ActionPayload {
            msg: "missing 'ssid' field".to_string(),
        })?;
        let raw_psk = extract_str(payload, "psk");
        let save = extract_bool(payload, "save").unwrap_or(false);
        let escaped_ssid = escape_wpa_string(raw_ssid)?;
        let escaped_psk = raw_psk.map(escape_wpa_string).transpose()?;

        let saved_networks = list_saved_networks(sock)
            .await
            .map_err(service_error_from_wifi_error)?;

        let id = if let Some(existing) = saved_networks
            .iter()
            .find(|network| network.ssid == raw_ssid)
        {
            if let Some(key) = escaped_psk.as_deref() {
                wpa_expect_ok(
                    sock,
                    &format!("SET_NETWORK {} psk \"{}\"", existing.id, key),
                )
                .await
                .map_err(service_error_from_wifi_error)?;
            }
            existing.id.clone()
        } else {
            let secure = scan_result_requires_psk(sock, raw_ssid).await;
            if secure && raw_psk.is_none() {
                return Err(ServiceError::ActionPayload {
                    msg: format!("network '{raw_ssid}' requires a psk"),
                });
            }

            let id_str = wpa_cmd(sock, "ADD_NETWORK")
                .await
                .map_err(service_error_from_wifi_error)?;
            let id =
                parse_network_id(&id_str, "ADD_NETWORK").map_err(service_error_from_wifi_error)?;

            wpa_expect_ok(sock, &format!("SET_NETWORK {id} ssid \"{escaped_ssid}\""))
                .await
                .map_err(service_error_from_wifi_error)?;

            if let Some(key) = escaped_psk.as_deref() {
                wpa_expect_ok(sock, &format!("SET_NETWORK {id} psk \"{key}\""))
                    .await
                    .map_err(service_error_from_wifi_error)?;
            } else {
                wpa_expect_ok(sock, &format!("SET_NETWORK {id} key_mgmt NONE"))
                    .await
                    .map_err(service_error_from_wifi_error)?;
            }

            id
        };

        if save {
            wpa_expect_ok(sock, &format!("ENABLE_NETWORK {id}"))
                .await
                .map_err(service_error_from_wifi_error)?;
        }

        wpa_expect_ok(sock, &format!("SELECT_NETWORK {id}"))
            .await
            .map_err(service_error_from_wifi_error)?;

        if save {
            wpa_expect_ok(sock, "SAVE_CONFIG")
                .await
                .map_err(service_error_from_wifi_error)?;
        }

        Ok(Value::Null)
    }

    async fn handle_disconnect(&mut self) -> Result<Value, ServiceError> {
        self.require_backend_ready().await?;
        let sock = self
            .cmd_sock
            .as_ref()
            .ok_or_else(|| ServiceError::Internal {
                msg: "wifi backend unavailable after socket setup".to_string(),
            })?;

        wpa_expect_ok(sock, "DISCONNECT")
            .await
            .map_err(service_error_from_wifi_error)?;

        Ok(Value::Null)
    }

    async fn handle_forget(&mut self, payload: &Value) -> Result<Value, ServiceError> {
        self.require_backend_ready().await?;
        let sock = self
            .cmd_sock
            .as_ref()
            .ok_or_else(|| ServiceError::Internal {
                msg: "wifi backend unavailable after socket setup".to_string(),
            })?;

        let ssid = extract_str(payload, "ssid").ok_or_else(|| ServiceError::ActionPayload {
            msg: "missing 'ssid' field".to_string(),
        })?;

        let list = wpa_cmd(sock, "LIST_NETWORKS")
            .await
            .map_err(service_error_from_wifi_error)?;

        for line in list.lines().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 && parts[1] == ssid {
                wpa_expect_ok(sock, &format!("REMOVE_NETWORK {}", parts[0]))
                    .await
                    .map_err(service_error_from_wifi_error)?;
                wpa_expect_ok(sock, "SAVE_CONFIG")
                    .await
                    .map_err(service_error_from_wifi_error)?;
                return Ok(Value::Null);
            }
        }

        Err(ServiceError::ActionPayload {
            msg: format!("network '{ssid}' not found"),
        })
    }

    async fn handle_set_enabled(&mut self, payload: &Value) -> Result<Value, ServiceError> {
        let enabled =
            extract_bool(payload, "enabled").ok_or_else(|| ServiceError::ActionPayload {
                msg: "missing 'enabled' bool field".to_string(),
            })?;

        run_rfkill(["unblock", "wifi"], ["block", "wifi"], enabled).await?;
        self.drop_sockets();
        Ok(Value::Null)
    }

    async fn handle_set_airplane_mode(&mut self, payload: &Value) -> Result<Value, ServiceError> {
        let enabled =
            extract_bool(payload, "enabled").ok_or_else(|| ServiceError::ActionPayload {
                msg: "missing 'enabled' bool field".to_string(),
            })?;

        run_rfkill(["unblock", "all"], ["block", "all"], !enabled).await?;
        self.drop_sockets();
        Ok(Value::Null)
    }
}

// ---------------------------------------------------------------------------
// Environment inspection and snapshot shaping
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct EnvironmentProbe {
    present: bool,
    interface_operstate: Option<String>,
    ctrl_socket_exists: bool,
    rfkill_available: bool,
    rfkill_soft_blocked: bool,
    rfkill_hard_blocked: bool,
    airplane_mode: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WifiAvailability {
    Ready,
    Disabled,
    Unavailable,
}

impl WifiAvailability {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Disabled => "disabled",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AvailabilityReason {
    #[default]
    Unknown,
    None,
    NoAdapter,
    RfkillSoftBlocked,
    RfkillHardBlocked,
    WpaSocketMissing,
    PermissionDenied,
    BackendError,
}

impl AvailabilityReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NoAdapter => "no_adapter",
            Self::RfkillSoftBlocked => "rfkill_soft_blocked",
            Self::RfkillHardBlocked => "rfkill_hard_blocked",
            Self::WpaSocketMissing => "wpa_socket_missing",
            Self::PermissionDenied => "permission_denied",
            Self::BackendError => "backend_error",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WifiConnectionState {
    Disconnected,
    Associating,
    Connected,
    #[default]
    Unknown,
}

impl WifiConnectionState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Associating => "associating",
            Self::Connected => "connected",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WifiScanState {
    #[default]
    Idle,
    Starting,
    Running,
}

impl WifiScanState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Running => "running",
        }
    }
}

fn derive_legacy_state(
    connection_state: WifiConnectionState,
    scan_state: WifiScanState,
) -> &'static str {
    if scan_state != WifiScanState::Idle {
        return "scanning";
    }

    match connection_state {
        WifiConnectionState::Associating => "associating",
        WifiConnectionState::Connected => "connected",
        WifiConnectionState::Disconnected => "disconnected",
        WifiConnectionState::Unknown => "unknown",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WifiStatus {
    present: bool,
    enabled: bool,
    availability: WifiAvailability,
    availability_reason: AvailabilityReason,
    interface_operstate: Option<String>,
    rfkill_available: bool,
    rfkill_soft_blocked: bool,
    rfkill_hard_blocked: bool,
    airplane_mode: bool,
}

impl Default for WifiStatus {
    fn default() -> Self {
        Self {
            present: false,
            enabled: false,
            availability: WifiAvailability::Unavailable,
            availability_reason: AvailabilityReason::Unknown,
            interface_operstate: None,
            rfkill_available: is_command_available("rfkill"),
            rfkill_soft_blocked: false,
            rfkill_hard_blocked: false,
            airplane_mode: false,
        }
    }
}

fn inspect_environment(iface: &str, ctrl_path: &str) -> EnvironmentProbe {
    let iface_path = PathBuf::from("/sys/class/net").join(iface);

    let rfkill = read_rfkill_probe(iface);

    EnvironmentProbe {
        present: iface_path.exists(),
        interface_operstate: read_trimmed(iface_path.join("operstate")),
        ctrl_socket_exists: Path::new(ctrl_path).exists(),
        rfkill_available: is_command_available("rfkill"),
        rfkill_soft_blocked: rfkill.wifi_soft_blocked,
        rfkill_hard_blocked: rfkill.wifi_hard_blocked,
        airplane_mode: rfkill.airplane_mode,
    }
}

fn classify_status(
    probe: &EnvironmentProbe,
    backend_ready: bool,
    ctrl_problem: Option<AvailabilityReason>,
) -> WifiStatus {
    let (availability, reason) = if !probe.present {
        (WifiAvailability::Unavailable, AvailabilityReason::NoAdapter)
    } else if probe.rfkill_hard_blocked {
        (
            WifiAvailability::Disabled,
            AvailabilityReason::RfkillHardBlocked,
        )
    } else if probe.rfkill_soft_blocked {
        (
            WifiAvailability::Disabled,
            AvailabilityReason::RfkillSoftBlocked,
        )
    } else if backend_ready {
        (WifiAvailability::Ready, AvailabilityReason::None)
    } else if let Some(problem) = ctrl_problem {
        (WifiAvailability::Unavailable, problem)
    } else if !probe.ctrl_socket_exists {
        (
            WifiAvailability::Unavailable,
            AvailabilityReason::WpaSocketMissing,
        )
    } else {
        (
            WifiAvailability::Unavailable,
            AvailabilityReason::BackendError,
        )
    };

    WifiStatus {
        present: probe.present,
        enabled: availability == WifiAvailability::Ready,
        availability,
        availability_reason: reason,
        interface_operstate: probe.interface_operstate.clone(),
        rfkill_available: probe.rfkill_available,
        rfkill_soft_blocked: probe.rfkill_soft_blocked,
        rfkill_hard_blocked: probe.rfkill_hard_blocked,
        airplane_mode: probe.airplane_mode,
    }
}

fn availability_reason_from_wifi_error(err: &WifiError) -> AvailabilityReason {
    match err {
        WifiError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound => {
            AvailabilityReason::WpaSocketMissing
        }
        WifiError::Io { source, .. } if source.kind() == io::ErrorKind::PermissionDenied => {
            AvailabilityReason::PermissionDenied
        }
        WifiError::Io { .. } | WifiError::Timeout | WifiError::CommandFailed { .. } => {
            AvailabilityReason::BackendError
        }
    }
}

#[derive(Clone, Debug, Default)]
struct RfkillProbe {
    wifi_soft_blocked: bool,
    wifi_hard_blocked: bool,
    airplane_mode: bool,
}

#[derive(Clone, Debug)]
struct RfkillEntry {
    kind: String,
    soft_blocked: bool,
    hard_blocked: bool,
}

fn read_rfkill_probe(iface: &str) -> RfkillProbe {
    let all_entries = read_all_rfkill_entries();
    if all_entries.is_empty() {
        return RfkillProbe::default();
    }

    let mut wifi_entries = read_iface_rfkill_entries(iface);
    if wifi_entries.is_empty() {
        wifi_entries = all_entries
            .iter()
            .filter(|entry| entry.kind == "wlan")
            .cloned()
            .collect();
    }

    let airplane_mode = all_entries
        .iter()
        .any(|entry| is_wireless_rfkill_type(&entry.kind))
        && all_entries
            .iter()
            .filter(|entry| is_wireless_rfkill_type(&entry.kind))
            .all(|entry| entry.soft_blocked || entry.hard_blocked);

    RfkillProbe {
        wifi_soft_blocked: wifi_entries.iter().any(|entry| entry.soft_blocked),
        wifi_hard_blocked: wifi_entries.iter().any(|entry| entry.hard_blocked),
        airplane_mode,
    }
}

fn read_all_rfkill_entries() -> Vec<RfkillEntry> {
    read_dir_entries("/sys/class/rfkill")
        .into_iter()
        .filter_map(|path| read_rfkill_entry(&path))
        .collect()
}

fn read_iface_rfkill_entries(iface: &str) -> Vec<RfkillEntry> {
    let mut paths = HashSet::new();

    for path in [
        PathBuf::from("/sys/class/net").join(iface).join("phy80211"),
        PathBuf::from("/sys/class/net")
            .join(iface)
            .join("device")
            .join("rfkill"),
    ] {
        for entry in read_dir_entries(&path) {
            let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with("rfkill") {
                continue;
            }
            paths.insert(entry);
        }
    }

    paths
        .into_iter()
        .filter_map(|path| read_rfkill_entry(&path))
        .collect()
}

fn read_rfkill_entry(path: &Path) -> Option<RfkillEntry> {
    let kind = read_trimmed(path.join("type"))?;
    let soft_blocked = read_trimmed(path.join("soft")).as_deref() == Some("1");
    let hard_blocked = read_trimmed(path.join("hard")).as_deref() == Some("1");

    Some(RfkillEntry {
        kind,
        soft_blocked,
        hard_blocked,
    })
}

fn is_wireless_rfkill_type(kind: &str) -> bool {
    matches!(kind, "wlan" | "bluetooth" | "uwb" | "wwan" | "nfc")
}

fn read_dir_entries<P: AsRef<Path>>(path: P) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let Ok(dir) = fs::read_dir(path) else {
        return entries;
    };

    for entry in dir.flatten() {
        entries.push(entry.path());
    }

    entries
}

fn read_trimmed<P: AsRef<Path>>(path: P) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
}

fn is_command_available(program: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&paths).any(|dir| {
        let candidate = dir.join(program);
        match fs::metadata(candidate) {
            Ok(meta) => meta.is_file() && (meta.permissions().mode() & 0o111 != 0),
            Err(_) => false,
        }
    })
}

// ---------------------------------------------------------------------------
// Socket helpers
// ---------------------------------------------------------------------------

async fn open_wpa_socket(ctrl_path: &str, kind: &str) -> Result<UnixDatagram, WifiError> {
    let uid = nix::unistd::getuid();
    let client_path = PathBuf::from(format!(
        "/run/user/{}/quicksov_wpa_ctrl_{}_{}",
        uid,
        std::process::id(),
        kind
    ));

    let _ = std::fs::remove_file(&client_path);

    let sock = UnixDatagram::bind(&client_path).map_err(|source| WifiError::Io {
        context: format!("bind {}", client_path.display()),
        source,
    })?;
    sock.connect(ctrl_path).map_err(|source| WifiError::Io {
        context: format!("connect {ctrl_path}"),
        source,
    })?;
    Ok(sock)
}

async fn wpa_cmd(sock: &UnixDatagram, cmd: &str) -> Result<String, WifiError> {
    sock.send(cmd.as_bytes())
        .await
        .map_err(|source| WifiError::Io {
            context: format!("send {cmd}"),
            source,
        })?;

    let mut buf = vec![0u8; 8192];
    let n = tokio::time::timeout(Duration::from_secs(3), sock.recv(&mut buf))
        .await
        .map_err(|_| WifiError::Timeout)?
        .map_err(|source| WifiError::Io {
            context: format!("recv reply for {cmd}"),
            source,
        })?;

    Ok(String::from_utf8_lossy(&buf[..n]).to_string())
}

async fn wpa_expect_ok(sock: &UnixDatagram, cmd: &str) -> Result<(), WifiError> {
    let reply = wpa_cmd(sock, cmd).await?;
    if reply.trim() == "OK" {
        return Ok(());
    }
    Err(WifiError::CommandFailed {
        cmd: cmd.to_string(),
        reply: reply.trim().to_string(),
    })
}

async fn wpa_request_scan(sock: &UnixDatagram) -> Result<ScanRequestOutcome, WifiError> {
    let reply = wpa_cmd(sock, "SCAN").await?;
    let trimmed = reply.trim();
    if scan_reply_is_accepted(trimmed) {
        return Ok(if trimmed == "FAIL-BUSY" {
            ScanRequestOutcome::Busy
        } else {
            ScanRequestOutcome::Started
        });
    }
    Err(WifiError::CommandFailed {
        cmd: "SCAN".to_string(),
        reply: trimmed.to_string(),
    })
}

async fn wpa_abort_scan(sock: &UnixDatagram) -> Result<(), WifiError> {
    wpa_expect_ok(sock, "ABORT_SCAN").await
}

fn scan_reply_is_accepted(reply: &str) -> bool {
    matches!(reply.trim(), "OK" | "FAIL-BUSY")
}

fn parse_network_id(reply: &str, cmd: &str) -> Result<String, WifiError> {
    let trimmed = reply.trim();
    if trimmed.parse::<u32>().is_ok() {
        return Ok(trimmed.to_string());
    }
    Err(WifiError::CommandFailed {
        cmd: cmd.to_string(),
        reply: trimmed.to_string(),
    })
}

// ---------------------------------------------------------------------------
// State reading
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct WifiReadState {
    connection_state: WifiConnectionState,
    status_scan_active: bool,
    ssid: Option<String>,
    bssid: Option<String>,
    rssi_dbm: Option<i64>,
    signal_pct: Option<i64>,
    frequency: Option<i64>,
    saved_networks: Vec<Value>,
    scan_results: Vec<Value>,
}

impl Default for WifiReadState {
    fn default() -> Self {
        Self {
            connection_state: WifiConnectionState::Unknown,
            status_scan_active: false,
            ssid: None,
            bssid: None,
            rssi_dbm: None,
            signal_pct: None,
            frequency: None,
            saved_networks: Vec::new(),
            scan_results: Vec::new(),
        }
    }
}

async fn read_full_state(sock: &UnixDatagram, _iface: &str) -> Result<WifiReadState, WifiError> {
    let status = wpa_cmd(sock, "STATUS").await?;
    let parsed = parse_status(&status);

    let wpa_state = parsed.get("wpa_state").cloned().unwrap_or_default();
    let connection_state = map_wpa_connection_state(&wpa_state);
    let ssid = parsed.get("ssid").cloned();
    let bssid = parsed.get("bssid").cloned();
    let frequency = parsed.get("freq").and_then(|s| s.parse().ok());

    let (rssi_dbm, signal_pct) = read_signal(sock).await;
    let saved_networks = read_saved_networks(sock).await;
    let scan_results = read_scan_results(sock).await;

    Ok(WifiReadState {
        connection_state,
        status_scan_active: wpa_state == "SCANNING",
        ssid,
        bssid,
        rssi_dbm,
        signal_pct,
        frequency,
        saved_networks,
        scan_results,
    })
}

fn build_wifi_snapshot(
    iface: &str,
    status: &WifiStatus,
    state: &WifiReadState,
    scan_state: WifiScanState,
    scan_started_at: Option<i64>,
    scan_finished_at: Option<i64>,
    scan_last_error: Option<&str>,
) -> Value {
    json_map([
        ("interface", Value::from(iface)),
        (
            "state",
            Value::from(derive_legacy_state(state.connection_state, scan_state)),
        ),
        (
            "connection_state",
            Value::from(state.connection_state.as_str()),
        ),
        ("scan_state", Value::from(scan_state.as_str())),
        ("scan_started_at", opt_i64_value(scan_started_at)),
        ("scan_finished_at", opt_i64_value(scan_finished_at)),
        ("scan_last_error", opt_str_value(scan_last_error)),
        ("present", Value::Bool(status.present)),
        ("enabled", Value::Bool(status.enabled)),
        ("availability", Value::from(status.availability.as_str())),
        (
            "availability_reason",
            Value::from(status.availability_reason.as_str()),
        ),
        (
            "interface_operstate",
            opt_str_value(status.interface_operstate.as_deref()),
        ),
        ("rfkill_available", Value::Bool(status.rfkill_available)),
        (
            "rfkill_soft_blocked",
            Value::Bool(status.rfkill_soft_blocked),
        ),
        (
            "rfkill_hard_blocked",
            Value::Bool(status.rfkill_hard_blocked),
        ),
        ("airplane_mode", Value::Bool(status.airplane_mode)),
        ("ssid", opt_str_value(state.ssid.as_deref())),
        ("bssid", opt_str_value(state.bssid.as_deref())),
        ("rssi_dbm", opt_i64_value(state.rssi_dbm)),
        ("signal_pct", opt_i64_value(state.signal_pct)),
        ("frequency", opt_i64_value(state.frequency)),
        ("saved_networks", Value::Array(state.saved_networks.clone())),
        ("scan_results", Value::Array(state.scan_results.clone())),
    ])
}

fn opt_str_value(value: Option<&str>) -> Value {
    match value {
        Some(text) => Value::from(text),
        None => Value::Null,
    }
}

fn opt_i64_value(value: Option<i64>) -> Value {
    match value {
        Some(num) => Value::from(num),
        None => Value::Null,
    }
}

fn unix_ms_now() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn map_wpa_connection_state(s: &str) -> WifiConnectionState {
    match s {
        "COMPLETED" => WifiConnectionState::Connected,
        "ASSOCIATING" | "ASSOCIATED" | "4WAY_HANDSHAKE" | "GROUP_HANDSHAKE" => {
            WifiConnectionState::Associating
        }
        "DISCONNECTED" | "INACTIVE" | "INTERFACE_DISABLED" | "SCANNING" => {
            WifiConnectionState::Disconnected
        }
        _ => WifiConnectionState::Unknown,
    }
}

fn parse_status(raw: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

async fn read_signal(sock: &UnixDatagram) -> (Option<i64>, Option<i64>) {
    let reply = match wpa_cmd(sock, "SIGNAL_POLL").await {
        Ok(r) => r,
        Err(_) => return (None, None),
    };
    let map = parse_status(&reply);
    let rssi: Option<i64> = map.get("RSSI").and_then(|s| s.parse().ok());
    let pct = rssi.map(|r| ((r + 100_i64) * 2_i64).clamp(0_i64, 100_i64));
    (rssi, pct)
}

async fn read_saved_networks(sock: &UnixDatagram) -> Vec<Value> {
    let rows = match list_saved_networks(sock).await {
        Ok(rows) => rows,
        Err(_) => return vec![],
    };

    rows.into_iter()
        .map(|row| {
            json_map([
                ("ssid", Value::from(row.ssid)),
                (
                    "priority",
                    Value::from(if row.flags.contains("[CURRENT]") {
                        1_i64
                    } else {
                        0_i64
                    }),
                ),
                ("auto", Value::Bool(true)),
            ])
        })
        .collect()
}

async fn read_scan_results(sock: &UnixDatagram) -> Vec<Value> {
    let reply = match wpa_cmd(sock, "SCAN_RESULTS").await {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    reply.lines().skip(1).filter_map(parse_scan_line).collect()
}

#[derive(Clone, Debug)]
struct SavedNetworkRow {
    id: String,
    ssid: String,
    flags: String,
}

async fn list_saved_networks(sock: &UnixDatagram) -> Result<Vec<SavedNetworkRow>, WifiError> {
    let reply = wpa_cmd(sock, "LIST_NETWORKS").await?;

    Ok(reply
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 2 {
                return None;
            }

            Some(SavedNetworkRow {
                id: parts[0].to_string(),
                ssid: parts[1].to_string(),
                flags: parts.get(3).copied().unwrap_or("").to_string(),
            })
        })
        .collect())
}

async fn scan_result_requires_psk(sock: &UnixDatagram, ssid: &str) -> bool {
    let results = read_scan_results(sock).await;
    for result in results {
        let Some(obj) = result.as_object() else {
            continue;
        };
        let Some(result_ssid) = obj.get("ssid").and_then(|value| value.as_str()) else {
            continue;
        };
        if result_ssid != ssid {
            continue;
        }

        let flags = obj
            .get("flags")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        return flags_require_psk(&flags);
    }

    false
}

fn flags_require_psk(flags: &[Value]) -> bool {
    flags.iter().any(|flag| {
        let Some(flag) = flag.as_str() else {
            return false;
        };
        flag.contains("WPA")
            || flag.contains("RSN")
            || flag.contains("PSK")
            || flag.contains("SAE")
            || flag.contains("WEP")
            || flag.contains("OWE")
            || flag.contains("802.1X")
    })
}

fn parse_scan_line(line: &str) -> Option<Value> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() < 5 {
        return None;
    }

    let bssid = parts[0];
    let frequency = parts[1].parse().unwrap_or(0);
    let rssi = parts[2].parse().unwrap_or(-100);
    let flags_str = parts[3];
    let ssid = parts[4];
    let signal_pct = ((rssi + 100_i64) * 2_i64).clamp(0_i64, 100_i64);

    let flags: Vec<Value> = flags_str
        .trim_matches(|c| c == '[' || c == ']')
        .split("][")
        .filter(|s| !s.is_empty())
        .map(Value::from)
        .collect();

    Some(json_map([
        ("ssid", Value::from(ssid)),
        ("bssid", Value::from(bssid)),
        ("rssi_dbm", Value::from(rssi)),
        ("signal_pct", Value::from(signal_pct)),
        ("flags", Value::Array(flags)),
        ("frequency", Value::from(frequency)),
    ]))
}

// ---------------------------------------------------------------------------
// rfkill commands
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Payload helpers
// ---------------------------------------------------------------------------

fn extract_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.as_object()?.get(key)?.as_str()
}

fn extract_bool(v: &Value, key: &str) -> Option<bool> {
    v.as_object()?.get(key)?.as_bool()
}

fn escape_wpa_string(value: &str) -> Result<String, ServiceError> {
    if value.contains(['\n', '\r', '\0']) {
        return Err(ServiceError::ActionPayload {
            msg: "wifi strings must not contain control newlines or NUL".to_string(),
        });
    }

    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(ch),
        }
    }
    Ok(escaped)
}

fn service_error_from_wifi_error(err: WifiError) -> ServiceError {
    ServiceError::Internal {
        msg: err.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum WifiError {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("wpa_supplicant command timeout")]
    Timeout,
    #[error("wpa_supplicant command failed: {cmd} -> {reply}")]
    CommandFailed { cmd: String, reply: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanRequestOutcome {
    Started,
    Busy,
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use tokio::net::UnixDatagram;

    use crate::config::{Config, NetworkConfig, ServicesConfig};

    use super::{
        build_wifi_snapshot, derive_legacy_state, iface_from_ctrl_path, resolve_wifi_control,
        wpa_abort_scan, wpa_request_scan, AvailabilityReason, WifiAvailability,
        WifiConnectionState, WifiReadState, WifiRuntime, WifiScanState, WifiStatus,
        DEFAULT_WIFI_INTERFACE,
    };

    async fn spawn_wpa_pair(
        expected_cmd: &'static str,
        reply: &'static str,
    ) -> (UnixDatagram, tokio::task::JoinHandle<()>) {
        let (server, client) = UnixDatagram::pair().expect("create unix datagram pair");
        let task = tokio::spawn(async move {
            let mut buf = [0u8; 256];
            let size = server.recv(&mut buf).await.expect("receive command");
            let command = String::from_utf8_lossy(&buf[..size]).to_string();
            assert_eq!(command, expected_cmd);
            server.send(reply.as_bytes()).await.expect("send reply");
        });

        (client, task)
    }

    #[test]
    fn explicit_wpa_ctrl_path_wins_over_interfaces() {
        let cfg = Config {
            daemon: Default::default(),
            screens: Default::default(),
            power: Default::default(),
            services: ServicesConfig {
                enabled: Vec::new(),
                weather: None,
                wallpaper: None,
                network: Some(NetworkConfig {
                    wifi_backend: Some("wpa_supplicant".to_string()),
                    wpa_ctrl_path: Some("/run/wpa_supplicant/wlp2s0".to_string()),
                    interfaces: Some(vec!["wlo1".to_string()]),
                }),
                audio: None,
                niri: None,
            },
        };

        let (ctrl_path, iface) = resolve_wifi_control(&cfg);
        assert_eq!(ctrl_path, "/run/wpa_supplicant/wlp2s0");
        assert_eq!(iface, "wlp2s0");
    }

    #[test]
    fn iface_is_derived_from_ctrl_path_basename() {
        assert_eq!(
            iface_from_ctrl_path("/run/wpa_supplicant/wlan42").as_deref(),
            Some("wlan42")
        );
        assert_eq!(iface_from_ctrl_path(""), None);
    }

    #[test]
    fn scan_busy_reply_is_treated_as_accepted() {
        assert!(super::scan_reply_is_accepted("OK"));
        assert!(super::scan_reply_is_accepted("FAIL-BUSY"));
        assert!(!super::scan_reply_is_accepted("FAIL"));
    }

    #[test]
    fn legacy_state_is_derived_from_orthogonal_states() {
        assert_eq!(
            derive_legacy_state(WifiConnectionState::Disconnected, WifiScanState::Idle),
            "disconnected"
        );
        assert_eq!(
            derive_legacy_state(WifiConnectionState::Associating, WifiScanState::Idle),
            "associating"
        );
        assert_eq!(
            derive_legacy_state(WifiConnectionState::Connected, WifiScanState::Idle),
            "connected"
        );
        assert_eq!(
            derive_legacy_state(WifiConnectionState::Connected, WifiScanState::Running),
            "scanning"
        );
        assert_eq!(
            derive_legacy_state(WifiConnectionState::Unknown, WifiScanState::Idle),
            "unknown"
        );
    }

    #[test]
    fn snapshot_exposes_connected_and_scanning_orthogonally() {
        let status = WifiStatus {
            present: true,
            enabled: true,
            availability: WifiAvailability::Ready,
            availability_reason: AvailabilityReason::None,
            interface_operstate: Some("up".to_string()),
            rfkill_available: true,
            rfkill_soft_blocked: false,
            rfkill_hard_blocked: false,
            airplane_mode: false,
        };
        let state = WifiReadState {
            connection_state: WifiConnectionState::Connected,
            ssid: Some("Office".to_string()),
            ..Default::default()
        };

        let snapshot = build_wifi_snapshot(
            "wlo1",
            &status,
            &state,
            WifiScanState::Running,
            Some(1_000),
            Some(2_000),
            Some("last error"),
        );

        assert_eq!(
            snapshot.get("connection_state").and_then(Value::as_str),
            Some("connected")
        );
        assert_eq!(
            snapshot.get("scan_state").and_then(Value::as_str),
            Some("running")
        );
        assert_eq!(
            snapshot.get("state").and_then(Value::as_str),
            Some("scanning")
        );
        assert_eq!(
            snapshot.get("scan_last_error").and_then(Value::as_str),
            Some("last error")
        );
    }

    #[test]
    fn scan_failure_records_last_error() {
        let mut runtime =
            WifiRuntime::new("/run/wpa_supplicant/wlo1".to_string(), "wlo1".to_string());
        runtime.mark_scan_runtime_failed("FAIL-BOOM".to_string());

        assert_eq!(runtime.scan_state, WifiScanState::Idle);
        assert_eq!(runtime.scan_last_error.as_deref(), Some("FAIL-BOOM"));
        assert!(runtime.scan_finished_at.is_some());
    }

    #[test]
    fn falls_back_to_default_interface_when_no_config_or_socket_exists() {
        let cfg = Config {
            daemon: Default::default(),
            screens: Default::default(),
            power: Default::default(),
            services: ServicesConfig::default(),
        };

        let (ctrl_path, iface) = resolve_wifi_control(&cfg);
        assert_eq!(iface, DEFAULT_WIFI_INTERFACE);
        assert!(ctrl_path.ends_with(&format!("/{DEFAULT_WIFI_INTERFACE}")));
    }

    #[tokio::test]
    async fn scan_start_accepts_ok_reply() {
        let (client, task) = spawn_wpa_pair("SCAN", "OK").await;
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime.cmd_sock = Some(client);

        let outcome = {
            let sock = runtime.cmd_sock.as_ref().expect("command socket");
            wpa_request_scan(sock).await
        };
        let reply = runtime.finish_scan_start(outcome);

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state, WifiScanState::Starting);
        assert!(runtime.scan_started_at.is_some());
        task.await.expect("server task");
    }

    #[tokio::test]
    async fn scan_start_accepts_fail_busy_reply() {
        let (client, task) = spawn_wpa_pair("SCAN", "FAIL-BUSY").await;
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime.cmd_sock = Some(client);

        let outcome = {
            let sock = runtime.cmd_sock.as_ref().expect("command socket");
            wpa_request_scan(sock).await
        };
        let reply = runtime.finish_scan_start(outcome);

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state, WifiScanState::Running);
        assert!(runtime.scan_started_at.is_some());
        task.await.expect("server task");
    }

    #[tokio::test]
    async fn scan_start_when_active_is_successful_noop() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime.scan_state = WifiScanState::Running;
        runtime.scan_started_at = Some(42);

        let reply = runtime.handle_scan_start().await;

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state, WifiScanState::Running);
        assert_eq!(runtime.scan_started_at, Some(42));
    }

    #[tokio::test]
    async fn scan_stop_when_idle_is_successful_noop() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());

        let reply = runtime.handle_scan_stop().await;

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state, WifiScanState::Idle);
        assert!(!runtime.scan_stop_requested);
    }

    #[tokio::test]
    async fn scan_stop_dispatches_abort_without_setting_error() {
        let (client, task) = spawn_wpa_pair("ABORT_SCAN", "OK").await;
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime.cmd_sock = Some(client);
        runtime.mark_scan_running();

        let result = {
            let sock = runtime.cmd_sock.as_ref().expect("command socket");
            wpa_abort_scan(sock).await
        };
        let reply = runtime.finish_scan_stop(result);

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state, WifiScanState::Running);
        assert!(runtime.scan_stop_requested);
        assert_eq!(runtime.scan_last_error, None);
        task.await.expect("server task");
    }

    #[test]
    fn status_poll_can_complete_user_requested_stop() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime.mark_scan_running();
        runtime.mark_scan_stop_requested();

        runtime.observe_status_scan(false);

        assert_eq!(runtime.scan_state, WifiScanState::Idle);
        assert!(runtime.scan_finished_at.is_some());
        assert!(!runtime.scan_stop_requested);
    }

    #[test]
    fn scan_failed_event_records_error_and_finishes_scan() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime.mark_scan_running();

        let relevant = runtime.observe_wpa_event("<3>CTRL-EVENT-SCAN-FAILED ret=-22 retry=1");

        assert!(relevant);
        assert_eq!(runtime.scan_state, WifiScanState::Idle);
        assert_eq!(
            runtime.scan_last_error.as_deref(),
            Some("<3>CTRL-EVENT-SCAN-FAILED ret=-22 retry=1")
        );
        assert!(runtime.scan_finished_at.is_some());
    }

    #[test]
    fn scan_failed_event_after_user_stop_does_not_record_error() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime.mark_scan_running();
        runtime.mark_scan_stop_requested();

        let relevant = runtime.observe_wpa_event("<3>CTRL-EVENT-SCAN-FAILED ret=-22 retry=1");

        assert!(relevant);
        assert_eq!(runtime.scan_state, WifiScanState::Idle);
        assert_eq!(runtime.scan_last_error, None);
        assert!(runtime.scan_finished_at.is_some());
    }
}
