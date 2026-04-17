// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `net.wifi` service — wpa_supplicant control socket backend.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use serde_json::Value;
use tokio::net::UnixDatagram;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::json_map;

/// Spawn the `net.wifi` service and return its [`ServiceHandle`].
pub fn spawn_wifi(cfg: &Config) -> ServiceHandle {
    let ctrl_path = cfg
        .services
        .network
        .as_ref()
        .and_then(|n| n.wpa_ctrl_path.as_deref())
        .unwrap_or("/run/wpa_supplicant/wlo1")
        .to_string();

    let iface = ctrl_path.rsplit('/').next().unwrap_or("wlo1").to_string();

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
    build_wifi_snapshot(iface, &WifiStatus::default(), &WifiStateSnapshot::default())
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
    let mut last_snapshot = runtime.refresh_snapshot().await;
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
                            if is_relevant_wpa_event(&msg) {
                                publish_snapshot(&mut runtime, &state_tx, &mut last_snapshot).await;
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
                    publish_snapshot(&mut runtime, &state_tx, &mut last_snapshot).await;
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
                    publish_snapshot(&mut runtime, &state_tx, &mut last_snapshot).await;
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
    let result = match req.action.as_str() {
        "scan" => runtime.handle_scan().await,
        "connect" => runtime.handle_connect(&req.payload).await,
        "disconnect" => runtime.handle_disconnect().await,
        "forget" => runtime.handle_forget(&req.payload).await,
        "set_enabled" => runtime.handle_set_enabled(&req.payload).await,
        "set_airplane_mode" => runtime.handle_set_airplane_mode(&req.payload).await,
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };

    publish_snapshot(runtime, state_tx, last_snapshot).await;
    req.reply.send(result).ok();
}

async fn publish_snapshot(
    runtime: &mut WifiRuntime,
    state_tx: &watch::Sender<Value>,
    last_snapshot: &mut Value,
) {
    let snapshot = runtime.refresh_snapshot().await;
    if snapshot != *last_snapshot {
        *last_snapshot = snapshot.clone();
        state_tx.send_replace(snapshot);
    }
}

fn is_relevant_wpa_event(message: &str) -> bool {
    message.contains("CTRL-EVENT-SCAN-RESULTS")
        || message.contains("CTRL-EVENT-CONNECTED")
        || message.contains("CTRL-EVENT-DISCONNECTED")
        || message.contains("CTRL-EVENT-SSID-TEMP-DISABLED")
        || message.contains("CTRL-EVENT-SSID-REENABLED")
}

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

struct WifiRuntime {
    ctrl_path: String,
    iface: String,
    cmd_sock: Option<UnixDatagram>,
    event_sock: Option<UnixDatagram>,
}

impl WifiRuntime {
    fn new(ctrl_path: String, iface: String) -> Self {
        Self {
            ctrl_path,
            iface,
            cmd_sock: None,
            event_sock: None,
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

    async fn refresh_snapshot(&mut self) -> Value {
        let probe = inspect_environment(&self.iface, &self.ctrl_path);
        let mut wifi_state = WifiStateSnapshot::default();
        let mut ctrl_problem = None;
        let mut backend_ready = false;

        if probe.present {
            match self.read_wpa_state().await {
                Ok(state) => {
                    wifi_state = state;
                    backend_ready = true;
                }
                Err(err) => {
                    ctrl_problem = Some(availability_reason_from_wifi_error(&err));
                    self.drop_sockets();
                }
            }
        } else {
            self.drop_sockets();
        }

        let status = classify_status(&probe, backend_ready, ctrl_problem);
        build_wifi_snapshot(&self.iface, &status, &wifi_state)
    }

    async fn read_wpa_state(&mut self) -> Result<WifiStateSnapshot, WifiError> {
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

    async fn handle_scan(&mut self) -> Result<Value, ServiceError> {
        self.require_backend_ready().await?;
        let sock = self
            .cmd_sock
            .as_ref()
            .ok_or_else(|| ServiceError::Internal {
                msg: "wifi backend unavailable after socket setup".to_string(),
            })?;

        wpa_expect_ok(sock, "SCAN")
            .await
            .map_err(service_error_from_wifi_error)?;

        Ok(Value::Null)
    }

    async fn handle_connect(&mut self, payload: &Value) -> Result<Value, ServiceError> {
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
        let psk = extract_str(payload, "psk");
        let save = extract_bool(payload, "save").unwrap_or(false);
        let ssid = escape_wpa_string(ssid)?;
        let psk = psk.map(escape_wpa_string).transpose()?;

        let id_str = wpa_cmd(sock, "ADD_NETWORK")
            .await
            .map_err(service_error_from_wifi_error)?;
        let id = parse_network_id(&id_str, "ADD_NETWORK").map_err(service_error_from_wifi_error)?;

        wpa_expect_ok(sock, &format!("SET_NETWORK {id} ssid \"{ssid}\""))
            .await
            .map_err(service_error_from_wifi_error)?;

        if let Some(key) = psk {
            wpa_expect_ok(sock, &format!("SET_NETWORK {id} psk \"{key}\""))
                .await
                .map_err(service_error_from_wifi_error)?;
        } else {
            wpa_expect_ok(sock, &format!("SET_NETWORK {id} key_mgmt NONE"))
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
struct WifiStateSnapshot {
    state: String,
    ssid: Option<String>,
    bssid: Option<String>,
    rssi_dbm: Option<i64>,
    signal_pct: Option<i64>,
    frequency: Option<i64>,
    saved_networks: Vec<Value>,
    scan_results: Vec<Value>,
}

impl Default for WifiStateSnapshot {
    fn default() -> Self {
        Self {
            state: "unknown".to_string(),
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

async fn read_full_state(
    sock: &UnixDatagram,
    _iface: &str,
) -> Result<WifiStateSnapshot, WifiError> {
    let status = wpa_cmd(sock, "STATUS").await?;
    let parsed = parse_status(&status);

    let wpa_state = parsed.get("wpa_state").cloned().unwrap_or_default();
    let state_str = map_wpa_state(&wpa_state).to_string();
    let ssid = parsed.get("ssid").cloned();
    let bssid = parsed.get("bssid").cloned();
    let frequency = parsed.get("freq").and_then(|s| s.parse().ok());

    let (rssi_dbm, signal_pct) = read_signal(sock).await;
    let saved_networks = read_saved_networks(sock).await;
    let scan_results = read_scan_results(sock).await;

    Ok(WifiStateSnapshot {
        state: state_str,
        ssid,
        bssid,
        rssi_dbm,
        signal_pct,
        frequency,
        saved_networks,
        scan_results,
    })
}

fn build_wifi_snapshot(iface: &str, status: &WifiStatus, state: &WifiStateSnapshot) -> Value {
    json_map([
        ("interface", Value::from(iface)),
        ("state", Value::from(state.state.as_str())),
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

fn map_wpa_state(s: &str) -> &'static str {
    match s {
        "COMPLETED" => "connected",
        "SCANNING" => "scanning",
        "ASSOCIATING" | "ASSOCIATED" | "4WAY_HANDSHAKE" | "GROUP_HANDSHAKE" => "associating",
        "DISCONNECTED" | "INACTIVE" | "INTERFACE_DISABLED" => "disconnected",
        _ => "unknown",
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
    let reply = match wpa_cmd(sock, "LIST_NETWORKS").await {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    reply
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let ssid = parts[1];
                let flags = parts.get(3).unwrap_or(&"");
                let is_current = flags.contains("[CURRENT]");
                Some(json_map([
                    ("ssid", Value::from(ssid)),
                    (
                        "priority",
                        Value::from(if is_current { 1_i64 } else { 0_i64 }),
                    ),
                    ("auto", Value::Bool(true)),
                ]))
            } else {
                None
            }
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
