// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `net.wifi` service — wpa_supplicant control socket backend.

use std::path::PathBuf;

use rmpv::Value;
use tokio::net::UnixDatagram;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::rmpv_map;

/// Spawn the `net.wifi` service and return its [`ServiceHandle`].
pub fn spawn_wifi(cfg: &Config) -> ServiceHandle {
    let ctrl_path = cfg
        .services
        .network
        .as_ref()
        .and_then(|n| n.wpa_ctrl_path.as_deref())
        .unwrap_or("/var/run/wpa_supplicant/wlo1")
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
    rmpv_map([
        ("interface", Value::from(iface)),
        ("state", Value::from("unknown")),
        ("ssid", Value::Nil),
        ("bssid", Value::Nil),
        ("rssi_dbm", Value::Nil),
        ("signal_pct", Value::Nil),
        ("frequency", Value::Nil),
        ("saved_networks", Value::Array(vec![])),
        ("scan_results", Value::Array(vec![])),
    ])
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
    loop {
        match connect_and_run(&mut request_rx, &state_tx, &ctrl_path, &iface).await {
            Ok(()) => break,
            Err(e) => {
                warn!(error = %e, "net.wifi wpa_supplicant connection failed; retrying in 5 s");
                state_tx.send_replace(unavailable_snapshot(&iface));
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
    info!("net.wifi service stopped");
}

async fn connect_and_run(
    request_rx: &mut mpsc::Receiver<ServiceRequest>,
    state_tx: &watch::Sender<Value>,
    ctrl_path: &str,
    iface: &str,
) -> Result<(), WifiError> {
    let sock = open_wpa_socket(ctrl_path).await?;

    // Attach to receive unsolicited events
    wpa_cmd(&sock, "ATTACH").await?;

    // Initial status
    let mut wifi_state = read_full_state(&sock, iface).await?;
    state_tx.send_replace(wifi_state.clone());

    let mut buf = vec![0u8; 4096];
    let poll_interval = tokio::time::interval(std::time::Duration::from_secs(10));
    tokio::pin!(poll_interval);

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                handle_request(req, &sock, state_tx, iface).await;
            }
            result = sock.recv(&mut buf) => {
                match result {
                    Ok(n) => {
                        let msg = String::from_utf8_lossy(&buf[..n]);
                        if msg.contains("CTRL-EVENT-SCAN-RESULTS")
                            || msg.contains("CTRL-EVENT-CONNECTED")
                            || msg.contains("CTRL-EVENT-DISCONNECTED")
                        {
                            if let Ok(new_state) = read_full_state(&sock, iface).await {
                                wifi_state = new_state;
                                state_tx.send_replace(wifi_state.clone());
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "wpa_supplicant socket recv error");
                        break;
                    }
                }
            }
            _ = poll_interval.tick() => {
                if let Ok(new_state) = read_full_state(&sock, iface).await {
                    if new_state != wifi_state {
                        wifi_state = new_state;
                        state_tx.send_replace(wifi_state.clone());
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Socket helpers
// ---------------------------------------------------------------------------

async fn open_wpa_socket(ctrl_path: &str) -> Result<UnixDatagram, WifiError> {
    let uid = nix::unistd::getuid();
    let client_path = PathBuf::from(format!(
        "/run/user/{}/quicksov_wpa_ctrl_{}",
        uid,
        std::process::id()
    ));
    // Remove stale socket if exists
    let _ = std::fs::remove_file(&client_path);

    let sock = UnixDatagram::bind(&client_path)
        .map_err(|e| WifiError::Io(format!("bind {}: {e}", client_path.display())))?;
    sock.connect(ctrl_path)
        .map_err(|e| WifiError::Io(format!("connect {ctrl_path}: {e}")))?;
    Ok(sock)
}

async fn wpa_cmd(sock: &UnixDatagram, cmd: &str) -> Result<String, WifiError> {
    sock.send(cmd.as_bytes())
        .await
        .map_err(|e| WifiError::Io(e.to_string()))?;

    let mut buf = vec![0u8; 8192];
    let n = tokio::time::timeout(std::time::Duration::from_secs(3), sock.recv(&mut buf))
        .await
        .map_err(|_| WifiError::Timeout)?
        .map_err(|e| WifiError::Io(e.to_string()))?;

    Ok(String::from_utf8_lossy(&buf[..n]).to_string())
}

// ---------------------------------------------------------------------------
// State reading
// ---------------------------------------------------------------------------

async fn read_full_state(sock: &UnixDatagram, iface: &str) -> Result<Value, WifiError> {
    let status = wpa_cmd(sock, "STATUS").await?;
    let parsed = parse_status(&status);

    let wpa_state = parsed.get("wpa_state").cloned().unwrap_or_default();
    let state_str = map_wpa_state(&wpa_state);
    let ssid = parsed.get("ssid").cloned();
    let bssid = parsed.get("bssid").cloned();
    let freq: Option<i64> = parsed.get("freq").and_then(|s| s.parse().ok());

    let (rssi, signal_pct) = read_signal(sock).await;
    let saved = read_saved_networks(sock).await;
    let scanned = read_scan_results(sock).await;

    Ok(build_wifi_snapshot(WifiParams {
        iface,
        state: state_str,
        ssid: &ssid,
        bssid: &bssid,
        rssi,
        signal_pct,
        freq,
        saved: &saved,
        scanned: &scanned,
    }))
}

struct WifiParams<'a> {
    iface: &'a str,
    state: &'a str,
    ssid: &'a Option<String>,
    bssid: &'a Option<String>,
    rssi: Option<i64>,
    signal_pct: Option<i64>,
    freq: Option<i64>,
    saved: &'a [Value],
    scanned: &'a [Value],
}

fn build_wifi_snapshot(p: WifiParams<'_>) -> Value {
    rmpv_map([
        ("interface", Value::from(p.iface)),
        ("state", Value::from(p.state)),
        ("ssid", opt_str_val(p.ssid)),
        ("bssid", opt_str_val(p.bssid)),
        ("rssi_dbm", opt_i64_val(p.rssi)),
        ("signal_pct", opt_i64_val(p.signal_pct)),
        ("frequency", opt_i64_val(p.freq)),
        ("saved_networks", Value::Array(p.saved.to_vec())),
        ("scan_results", Value::Array(p.scanned.to_vec())),
    ])
}

fn opt_str_val(v: &Option<String>) -> Value {
    match v {
        Some(s) => Value::from(s.as_str()),
        None => Value::Nil,
    }
}

fn opt_i64_val(v: Option<i64>) -> Value {
    match v {
        Some(n) => Value::from(n),
        None => Value::Nil,
    }
}

fn map_wpa_state(s: &str) -> &'static str {
    match s {
        "COMPLETED" => "connected",
        "SCANNING" => "scanning",
        "ASSOCIATING" | "ASSOCIATED" => "associating",
        "DISCONNECTED" | "INACTIVE" => "disconnected",
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
    let pct = rssi.map(|r| ((r + 100) * 2).clamp(0, 100));
    (rssi, pct)
}

async fn read_saved_networks(sock: &UnixDatagram) -> Vec<Value> {
    let reply = match wpa_cmd(sock, "LIST_NETWORKS").await {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    // First line is header: "network id / ssid / bssid / flags"
    reply
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let ssid = parts[1];
                let flags = parts.get(3).unwrap_or(&"");
                let is_current = flags.contains("[CURRENT]");
                Some(rmpv_map([
                    ("ssid", Value::from(ssid)),
                    (
                        "priority",
                        Value::from(if is_current { 1_i64 } else { 0_i64 }),
                    ),
                    ("auto", Value::Boolean(true)),
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
    // Header: bssid / frequency / signal level / flags / ssid
    reply.lines().skip(1).filter_map(parse_scan_line).collect()
}

fn parse_scan_line(line: &str) -> Option<Value> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() < 5 {
        return None;
    }
    let bssid = parts[0];
    let freq: i64 = parts[1].parse().unwrap_or(0);
    let rssi: i64 = parts[2].parse().unwrap_or(-100);
    let flags_str = parts[3];
    let ssid = parts[4];
    let signal_pct = ((rssi + 100) * 2).clamp(0, 100);

    let flags: Vec<Value> = flags_str
        .trim_matches(|c| c == '[' || c == ']')
        .split("][")
        .filter(|s| !s.is_empty())
        .map(Value::from)
        .collect();

    Some(rmpv_map([
        ("ssid", Value::from(ssid)),
        ("bssid", Value::from(bssid)),
        ("rssi_dbm", Value::from(rssi)),
        ("signal_pct", Value::from(signal_pct)),
        ("flags", Value::Array(flags)),
        ("frequency", Value::from(freq)),
    ]))
}

// ---------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------

async fn handle_request(
    req: ServiceRequest,
    sock: &UnixDatagram,
    state_tx: &watch::Sender<Value>,
    iface: &str,
) {
    let result = match req.action.as_str() {
        "scan" => handle_scan(sock).await,
        "connect" => handle_connect(&req.payload, sock).await,
        "disconnect" => handle_disconnect(sock).await,
        "forget" => handle_forget(&req.payload, sock).await,
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    // Refresh state after any successful command
    if result.is_ok() {
        if let Ok(new_state) = read_full_state(sock, iface).await {
            state_tx.send_replace(new_state);
        }
    }
    req.reply.send(result).ok();
}

async fn handle_scan(sock: &UnixDatagram) -> Result<Value, ServiceError> {
    wpa_cmd(sock, "SCAN")
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Nil)
}

async fn handle_connect(payload: &Value, sock: &UnixDatagram) -> Result<Value, ServiceError> {
    let ssid = extract_str(payload, "ssid").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'ssid' field".to_string(),
    })?;
    let psk = extract_str(payload, "psk");
    let save = extract_bool(payload, "save").unwrap_or(false);

    // Add network
    let id_str = wpa_cmd(sock, "ADD_NETWORK")
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    let id = id_str.trim();

    wpa_cmd(sock, &format!("SET_NETWORK {id} ssid \"{ssid}\""))
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;

    if let Some(key) = psk {
        wpa_cmd(sock, &format!("SET_NETWORK {id} psk \"{key}\""))
            .await
            .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    } else {
        wpa_cmd(sock, &format!("SET_NETWORK {id} key_mgmt NONE"))
            .await
            .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    }

    wpa_cmd(sock, &format!("SELECT_NETWORK {id}"))
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;

    if save {
        wpa_cmd(sock, "SAVE_CONFIG")
            .await
            .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    }

    Ok(Value::Nil)
}

async fn handle_disconnect(sock: &UnixDatagram) -> Result<Value, ServiceError> {
    wpa_cmd(sock, "DISCONNECT")
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Nil)
}

async fn handle_forget(payload: &Value, sock: &UnixDatagram) -> Result<Value, ServiceError> {
    let ssid = extract_str(payload, "ssid").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'ssid' field".to_string(),
    })?;

    // List networks and find the one matching ssid
    let list = wpa_cmd(sock, "LIST_NETWORKS")
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;

    for line in list.lines().skip(1) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 && parts[1] == ssid {
            wpa_cmd(sock, &format!("REMOVE_NETWORK {}", parts[0]))
                .await
                .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
            wpa_cmd(sock, "SAVE_CONFIG")
                .await
                .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
            return Ok(Value::Nil);
        }
    }

    Err(ServiceError::ActionPayload {
        msg: format!("network '{ssid}' not found"),
    })
}

// ---------------------------------------------------------------------------
// Payload helpers
// ---------------------------------------------------------------------------

fn extract_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    if let Value::Map(pairs) = v {
        for (k, val) in pairs {
            if k.as_str() == Some(key) {
                return val.as_str();
            }
        }
    }
    None
}

fn extract_bool(v: &Value, key: &str) -> Option<bool> {
    if let Value::Map(pairs) = v {
        for (k, val) in pairs {
            if k.as_str() == Some(key) {
                return val.as_bool();
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum WifiError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("wpa_supplicant command timeout")]
    Timeout,
}
