// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::Value;

use crate::util::json_map;

use super::manual::ManualConnectTracker;
use super::model::{derive_legacy_state, WifiReadState, WifiStatus};
use super::scan::ScanTracker;

pub(super) fn build_wifi_snapshot(
    iface: &str,
    status: &WifiStatus,
    state: &WifiReadState,
    scan: &ScanTracker,
    manual_connect: &ManualConnectTracker,
) -> Value {
    WifiSnapshot {
        iface,
        status,
        state,
        scan,
        manual_connect,
    }
    .into()
}

struct WifiSnapshot<'a> {
    iface: &'a str,
    status: &'a WifiStatus,
    state: &'a WifiReadState,
    scan: &'a ScanTracker,
    manual_connect: &'a ManualConnectTracker,
}

impl From<WifiSnapshot<'_>> for Value {
    fn from(snapshot: WifiSnapshot<'_>) -> Self {
        json_map([
            ("interface", Value::from(snapshot.iface)),
            (
                "state",
                Value::from(derive_legacy_state(
                    snapshot.state.connection_state,
                    snapshot.scan.state(),
                )),
            ),
            (
                "connection_state",
                Value::from(snapshot.state.connection_state.as_str()),
            ),
            ("scan_state", Value::from(snapshot.scan.state().as_str())),
            ("scan_started_at", opt_i64_value(snapshot.scan.started_at())),
            (
                "scan_finished_at",
                opt_i64_value(snapshot.scan.finished_at()),
            ),
            ("scan_last_error", opt_str_value(snapshot.scan.last_error())),
            (
                "manual_connect_state",
                Value::from(snapshot.manual_connect.state().as_str()),
            ),
            (
                "manual_connect_ssid",
                opt_str_value(snapshot.manual_connect.target_ssid()),
            ),
            (
                "manual_connect_reason",
                Value::from(snapshot.manual_connect.reason().as_str()),
            ),
            (
                "manual_connect_started_at",
                opt_i64_value(snapshot.manual_connect.started_at()),
            ),
            ("present", Value::Bool(snapshot.status.present)),
            ("enabled", Value::Bool(snapshot.status.enabled)),
            (
                "availability",
                Value::from(snapshot.status.availability.as_str()),
            ),
            (
                "availability_reason",
                Value::from(snapshot.status.availability_reason.as_str()),
            ),
            (
                "interface_operstate",
                opt_str_value(snapshot.status.interface_operstate.as_deref()),
            ),
            (
                "rfkill_available",
                Value::Bool(snapshot.status.rfkill_available),
            ),
            (
                "rfkill_soft_blocked",
                Value::Bool(snapshot.status.rfkill_soft_blocked),
            ),
            (
                "rfkill_hard_blocked",
                Value::Bool(snapshot.status.rfkill_hard_blocked),
            ),
            ("airplane_mode", Value::Bool(snapshot.status.airplane_mode)),
            (
                "network_id",
                opt_str_value(snapshot.state.network_id.as_deref()),
            ),
            ("ssid", opt_str_value(snapshot.state.ssid.as_deref())),
            ("bssid", opt_str_value(snapshot.state.bssid.as_deref())),
            ("rssi_dbm", opt_i64_value(snapshot.state.rssi_dbm)),
            ("signal_pct", opt_i64_value(snapshot.state.signal_pct)),
            ("frequency", opt_i64_value(snapshot.state.frequency)),
            (
                "saved_networks",
                Value::Array(snapshot.state.saved_networks.clone()),
            ),
            (
                "scan_results",
                Value::Array(snapshot.state.scan_results.clone()),
            ),
        ])
    }
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
