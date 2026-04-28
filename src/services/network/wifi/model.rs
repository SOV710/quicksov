// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WifiAvailability {
    Ready,
    Disabled,
    Unavailable,
}

impl WifiAvailability {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Disabled => "disabled",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum AvailabilityReason {
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
    pub(super) fn as_str(self) -> &'static str {
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
pub(super) enum WifiConnectionState {
    Disconnected,
    Associating,
    Connected,
    #[default]
    Unknown,
}

impl WifiConnectionState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Associating => "associating",
            Self::Connected => "connected",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WifiScanState {
    #[default]
    Idle,
    Starting,
    Running,
}

impl WifiScanState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Running => "running",
        }
    }
}

pub(super) fn derive_legacy_state(
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
pub(super) struct WifiStatus {
    pub(super) present: bool,
    pub(super) enabled: bool,
    pub(super) availability: WifiAvailability,
    pub(super) availability_reason: AvailabilityReason,
    pub(super) interface_operstate: Option<String>,
    pub(super) rfkill_available: bool,
    pub(super) rfkill_soft_blocked: bool,
    pub(super) rfkill_hard_blocked: bool,
    pub(super) airplane_mode: bool,
}

impl Default for WifiStatus {
    fn default() -> Self {
        Self {
            present: false,
            enabled: false,
            availability: WifiAvailability::Unavailable,
            availability_reason: AvailabilityReason::Unknown,
            interface_operstate: None,
            rfkill_available: false,
            rfkill_soft_blocked: false,
            rfkill_hard_blocked: false,
            airplane_mode: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct WifiReadState {
    pub(super) connection_state: WifiConnectionState,
    pub(super) status_scan_active: bool,
    pub(super) network_id: Option<String>,
    pub(super) ssid: Option<String>,
    pub(super) bssid: Option<String>,
    pub(super) rssi_dbm: Option<i64>,
    pub(super) signal_pct: Option<i64>,
    pub(super) frequency: Option<i64>,
    pub(super) saved_networks: Vec<Value>,
    pub(super) scan_results: Vec<Value>,
}

impl Default for WifiReadState {
    fn default() -> Self {
        Self {
            connection_state: WifiConnectionState::Unknown,
            status_scan_active: false,
            network_id: None,
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

#[derive(Clone, Debug)]
pub(super) struct SavedNetworkRow {
    pub(super) id: String,
    pub(super) ssid: String,
    pub(super) flags: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScanRequestOutcome {
    Started,
    Busy,
}
