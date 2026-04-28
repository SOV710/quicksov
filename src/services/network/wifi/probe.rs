// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::error::WifiError;
use super::model::{AvailabilityReason, WifiAvailability, WifiStatus};

#[derive(Clone, Debug)]
pub(super) struct EnvironmentProbe {
    pub(super) present: bool,
    pub(super) interface_operstate: Option<String>,
    pub(super) ctrl_socket_exists: bool,
    pub(super) rfkill_available: bool,
    pub(super) rfkill_soft_blocked: bool,
    pub(super) rfkill_hard_blocked: bool,
    pub(super) airplane_mode: bool,
}

pub(super) fn inspect_environment(iface: &str, ctrl_path: &str) -> EnvironmentProbe {
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

pub(super) fn classify_status(
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

pub(super) fn availability_reason_from_wifi_error(err: &WifiError) -> AvailabilityReason {
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
