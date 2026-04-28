// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `net.wifi` service — wpa_supplicant control socket backend.

use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::warn;

use crate::bus::ServiceHandle;
use crate::config::Config;

mod client;
mod command;
mod error;
mod model;
mod probe;
mod runtime;
mod scan;
mod service;
mod snapshot;

use model::{WifiReadState, WifiStatus};
use scan::ScanTracker;
use snapshot::build_wifi_snapshot;

const DEFAULT_WIFI_BACKEND: &str = "wpa_supplicant";
const DEFAULT_WPA_CTRL_DIR: &str = "/run/wpa_supplicant";
pub(super) const DEFAULT_WIFI_INTERFACE: &str = "wlo1";

/// Spawn the `net.wifi` service and return its [`ServiceHandle`].
pub fn spawn_wifi(cfg: &Config) -> ServiceHandle {
    let (ctrl_path, iface) = resolve_wifi_control(cfg);

    let initial = unavailable_snapshot(&iface);
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(service::run(request_rx, state_tx, ctrl_path, iface));

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
        &ScanTracker::default(),
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

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use tokio::net::UnixDatagram;

    use crate::config::{Config, NetworkConfig, ServicesConfig};

    use super::client::{scan_reply_is_accepted, wpa_abort_scan, wpa_request_scan};
    use super::model::{
        derive_legacy_state, AvailabilityReason, WifiAvailability, WifiConnectionState,
        WifiReadState, WifiScanState, WifiStatus,
    };
    use super::runtime::WifiRuntime;
    use super::scan::ScanTracker;
    use super::snapshot::build_wifi_snapshot;
    use super::{iface_from_ctrl_path, resolve_wifi_control, DEFAULT_WIFI_INTERFACE};

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
        assert!(scan_reply_is_accepted("OK"));
        assert!(scan_reply_is_accepted("FAIL-BUSY"));
        assert!(!scan_reply_is_accepted("FAIL"));
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
        let mut scan = ScanTracker::default();
        scan.set_for_test(
            WifiScanState::Running,
            Some(1_000),
            Some(2_000),
            Some("last error".to_string()),
            false,
        );

        let snapshot = build_wifi_snapshot("wlo1", &status, &state, &scan);

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

        assert_eq!(runtime.scan_state(), WifiScanState::Idle);
        assert_eq!(runtime.scan_last_error(), Some("FAIL-BOOM"));
        assert!(runtime.scan_finished_at().is_some());
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

        let outcome = wpa_request_scan(&client).await;
        let reply = runtime.finish_scan_start(outcome);

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state(), WifiScanState::Starting);
        assert!(runtime.scan_started_at().is_some());
        task.await.expect("server task");
    }

    #[tokio::test]
    async fn scan_start_accepts_fail_busy_reply() {
        let (client, task) = spawn_wpa_pair("SCAN", "FAIL-BUSY").await;
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());

        let outcome = wpa_request_scan(&client).await;
        let reply = runtime.finish_scan_start(outcome);

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state(), WifiScanState::Running);
        assert!(runtime.scan_started_at().is_some());
        task.await.expect("server task");
    }

    #[tokio::test]
    async fn scan_start_when_active_is_successful_noop() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime
            .scan
            .set_for_test(WifiScanState::Running, Some(42), None, None, false);

        let reply = runtime.handle_scan_start().await;

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state(), WifiScanState::Running);
        assert_eq!(runtime.scan_started_at(), Some(42));
    }

    #[tokio::test]
    async fn scan_stop_when_idle_is_successful_noop() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());

        let reply = runtime.handle_scan_stop().await;

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state(), WifiScanState::Idle);
        assert!(!runtime.scan_stop_requested());
    }

    #[tokio::test]
    async fn scan_stop_dispatches_abort_without_setting_error() {
        let (client, task) = spawn_wpa_pair("ABORT_SCAN", "OK").await;
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime
            .scan
            .set_for_test(WifiScanState::Running, None, None, None, false);

        let result = wpa_abort_scan(&client).await;
        let reply = runtime.finish_scan_stop(result);

        assert!(reply.is_ok());
        assert_eq!(runtime.scan_state(), WifiScanState::Running);
        assert!(runtime.scan_stop_requested());
        assert_eq!(runtime.scan_last_error(), None);
        task.await.expect("server task");
    }

    #[test]
    fn status_poll_can_complete_user_requested_stop() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime
            .scan
            .set_for_test(WifiScanState::Running, None, None, None, true);

        runtime.observe_status_scan(false);

        assert_eq!(runtime.scan_state(), WifiScanState::Idle);
        assert!(runtime.scan_finished_at().is_some());
        assert!(!runtime.scan_stop_requested());
    }

    #[test]
    fn scan_failed_event_records_error_and_finishes_scan() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime
            .scan
            .set_for_test(WifiScanState::Running, None, None, None, false);

        let relevant = runtime.observe_wpa_event("<3>CTRL-EVENT-SCAN-FAILED ret=-22 retry=1");

        assert!(relevant);
        assert_eq!(runtime.scan_state(), WifiScanState::Idle);
        assert_eq!(
            runtime.scan_last_error(),
            Some("<3>CTRL-EVENT-SCAN-FAILED ret=-22 retry=1")
        );
        assert!(runtime.scan_finished_at().is_some());
    }

    #[test]
    fn scan_failed_event_after_user_stop_does_not_record_error() {
        let mut runtime = WifiRuntime::new("/unused".to_string(), "lo".to_string());
        runtime
            .scan
            .set_for_test(WifiScanState::Running, None, None, None, true);

        let relevant = runtime.observe_wpa_event("<3>CTRL-EVENT-SCAN-FAILED ret=-22 retry=1");

        assert!(relevant);
        assert_eq!(runtime.scan_state(), WifiScanState::Idle);
        assert_eq!(runtime.scan_last_error(), None);
        assert!(runtime.scan_finished_at().is_some());
    }
}
