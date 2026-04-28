// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use tokio::net::UnixDatagram;

use crate::util::json_map;

use super::error::WifiError;
use super::model::{SavedNetworkRow, ScanRequestOutcome, WifiConnectionState, WifiReadState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WpaNetworkSecurity {
    Open,
    Psk,
    Sae,
    SaeTransition,
    Unsupported,
}

impl WpaNetworkSecurity {
    pub(super) fn requires_passphrase(self) -> bool {
        matches!(self, Self::Psk | Self::Sae | Self::SaeTransition)
    }
}

pub(super) struct WpaCtrlClient {
    ctrl_path: String,
    cmd_sock: Option<UnixDatagram>,
    event_sock: Option<UnixDatagram>,
}

impl WpaCtrlClient {
    pub(super) fn new(ctrl_path: String) -> Self {
        Self {
            ctrl_path,
            cmd_sock: None,
            event_sock: None,
        }
    }

    pub(super) fn ctrl_path(&self) -> &str {
        &self.ctrl_path
    }

    pub(super) fn has_command_socket(&self) -> bool {
        self.cmd_sock.is_some()
    }

    pub(super) fn has_event_socket(&self) -> bool {
        self.event_sock.is_some()
    }

    pub(super) fn take_event_socket(&mut self) -> Option<UnixDatagram> {
        self.event_sock.take()
    }

    pub(super) fn restore_event_socket(&mut self, sock: UnixDatagram) {
        self.event_sock = Some(sock);
    }

    pub(super) fn drop_sockets(&mut self) {
        self.cmd_sock = None;
        self.event_sock = None;
    }

    pub(super) async fn ensure_command_socket(&mut self) -> Result<(), WifiError> {
        if self.cmd_sock.is_some() {
            return Ok(());
        }

        let sock = open_wpa_socket(&self.ctrl_path, "cmd").await?;
        self.cmd_sock = Some(sock);
        Ok(())
    }

    pub(super) async fn try_attach_event_socket(&mut self) -> Result<(), WifiError> {
        let sock = open_wpa_socket(&self.ctrl_path, "evt").await?;
        wpa_expect_ok(&sock, "ATTACH").await?;
        self.event_sock = Some(sock);
        Ok(())
    }

    pub(super) async fn read_state(&mut self) -> Result<WifiReadState, WifiError> {
        self.ensure_command_socket().await?;
        let sock = self.command_socket()?;
        read_full_state(sock).await
    }

    pub(super) async fn request_scan(&self) -> Result<ScanRequestOutcome, WifiError> {
        wpa_request_scan(self.command_socket()?).await
    }

    pub(super) async fn abort_scan(&self) -> Result<(), WifiError> {
        wpa_abort_scan(self.command_socket()?).await
    }

    pub(super) async fn list_saved_networks(&self) -> Result<Vec<SavedNetworkRow>, WifiError> {
        list_saved_networks(self.command_socket()?).await
    }

    pub(super) async fn scan_result_security(&self, ssid: &str) -> Option<WpaNetworkSecurity> {
        scan_result_security(self.command_socket().ok(), ssid).await
    }

    pub(super) async fn add_network(&self) -> Result<String, WifiError> {
        let id_str = wpa_cmd(self.command_socket()?, "ADD_NETWORK").await?;
        parse_network_id(&id_str, "ADD_NETWORK")
    }

    pub(super) async fn set_network_ssid(
        &self,
        id: &str,
        escaped_ssid: &str,
    ) -> Result<(), WifiError> {
        wpa_expect_ok(
            self.command_socket()?,
            &format!("SET_NETWORK {id} ssid \"{escaped_ssid}\""),
        )
        .await
    }

    pub(super) async fn set_network_psk(
        &self,
        id: &str,
        escaped_psk: &str,
    ) -> Result<(), WifiError> {
        wpa_expect_ok(
            self.command_socket()?,
            &format!("SET_NETWORK {id} psk \"{escaped_psk}\""),
        )
        .await
    }

    pub(super) async fn set_network_sae_password(
        &self,
        id: &str,
        escaped_password: &str,
    ) -> Result<(), WifiError> {
        wpa_expect_ok(
            self.command_socket()?,
            &format!("SET_NETWORK {id} sae_password \"{escaped_password}\""),
        )
        .await
    }

    pub(super) async fn set_network_key_mgmt(
        &self,
        id: &str,
        key_mgmt: &str,
    ) -> Result<(), WifiError> {
        wpa_expect_ok(
            self.command_socket()?,
            &format!("SET_NETWORK {id} key_mgmt {key_mgmt}"),
        )
        .await
    }

    pub(super) async fn set_network_ieee80211w(
        &self,
        id: &str,
        value: u8,
    ) -> Result<(), WifiError> {
        wpa_expect_ok(
            self.command_socket()?,
            &format!("SET_NETWORK {id} ieee80211w {value}"),
        )
        .await
    }

    pub(super) async fn set_network_open(&self, id: &str) -> Result<(), WifiError> {
        wpa_expect_ok(
            self.command_socket()?,
            &format!("SET_NETWORK {id} key_mgmt NONE"),
        )
        .await
    }

    pub(super) async fn enable_network(&self, id: &str) -> Result<(), WifiError> {
        wpa_expect_ok(self.command_socket()?, &format!("ENABLE_NETWORK {id}")).await
    }

    pub(super) async fn select_network(&self, id: &str) -> Result<(), WifiError> {
        wpa_expect_ok(self.command_socket()?, &format!("SELECT_NETWORK {id}")).await
    }

    pub(super) async fn set_network_disabled(
        &self,
        id: &str,
        disabled: bool,
    ) -> Result<(), WifiError> {
        let value = if disabled { 1 } else { 0 };
        wpa_expect_ok(
            self.command_socket()?,
            &format!("SET_NETWORK {id} disabled {value}"),
        )
        .await
    }

    pub(super) async fn save_config(&self) -> Result<(), WifiError> {
        wpa_expect_ok(self.command_socket()?, "SAVE_CONFIG").await
    }

    pub(super) async fn remove_network(&self, id: &str) -> Result<(), WifiError> {
        wpa_expect_ok(self.command_socket()?, &format!("REMOVE_NETWORK {id}")).await
    }

    pub(super) async fn disconnect(&self) -> Result<(), WifiError> {
        wpa_expect_ok(self.command_socket()?, "DISCONNECT").await
    }

    fn command_socket(&self) -> Result<&UnixDatagram, WifiError> {
        self.cmd_sock.as_ref().ok_or_else(|| WifiError::Io {
            context: "missing wpa_supplicant command socket after connect".to_string(),
            source: io::Error::new(io::ErrorKind::NotConnected, "command socket unavailable"),
        })
    }
}

async fn open_wpa_socket(ctrl_path: &str, kind: &str) -> Result<UnixDatagram, WifiError> {
    let uid = nix::unistd::getuid();
    let client_path = PathBuf::from(format!(
        "/run/user/{}/quicksov_wpa_ctrl_{}_{}",
        uid,
        std::process::id(),
        kind
    ));

    let _ = fs::remove_file(&client_path);

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

pub(super) async fn wpa_request_scan(sock: &UnixDatagram) -> Result<ScanRequestOutcome, WifiError> {
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

pub(super) async fn wpa_abort_scan(sock: &UnixDatagram) -> Result<(), WifiError> {
    wpa_expect_ok(sock, "ABORT_SCAN").await
}

pub(super) fn scan_reply_is_accepted(reply: &str) -> bool {
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

async fn read_full_state(sock: &UnixDatagram) -> Result<WifiReadState, WifiError> {
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

async fn scan_result_security(
    sock: Option<&UnixDatagram>,
    ssid: &str,
) -> Option<WpaNetworkSecurity> {
    let sock = sock?;

    let results = read_scan_results(sock).await;
    for result in results {
        let Some(obj) = result.as_object() else {
            continue;
        };
        let Some(result_ssid) = obj.get("ssid").and_then(Value::as_str) else {
            continue;
        };
        if result_ssid != ssid {
            continue;
        }

        let flags = obj
            .get("flags")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        return Some(security_from_flags(&flags));
    }

    None
}

pub(super) fn security_from_flags(flags: &[Value]) -> WpaNetworkSecurity {
    let has_sae = flags_contain(flags, "SAE");
    let has_psk = flags_contain(flags, "PSK");
    let has_eap = flags_contain(flags, "802.1X") || flags_contain(flags, "EAP");
    let has_wep = flags_contain(flags, "WEP");
    let has_owe = flags_contain(flags, "OWE");

    if has_sae && has_psk {
        return WpaNetworkSecurity::SaeTransition;
    }
    if has_sae {
        return WpaNetworkSecurity::Sae;
    }
    if has_psk {
        return WpaNetworkSecurity::Psk;
    }
    if has_owe || has_eap || has_wep {
        return WpaNetworkSecurity::Unsupported;
    }
    WpaNetworkSecurity::Open
}

fn flags_contain(flags: &[Value], needle: &str) -> bool {
    flags.iter().any(|flag| {
        let Some(flag) = flag.as_str() else {
            return false;
        };
        flag.contains(needle)
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
