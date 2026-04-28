// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use super::model::{WifiConnectionState, WifiReadState};

pub(super) const DEFAULT_MANUAL_CONNECT_TIMEOUT_MS: i64 = 30_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ManualConnectState {
    #[default]
    Idle,
    Connecting,
    Failed,
}

impl ManualConnectState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Connecting => "connecting",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ManualConnectReason {
    #[default]
    None,
    AuthFailed,
    Timeout,
    BackendError,
}

impl ManualConnectReason {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::AuthFailed => "auth_failed",
            Self::Timeout => "timeout",
            Self::BackendError => "backend_error",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ManualConnectOutcome {
    None,
    Succeeded { restore_enabled_ids: Vec<String> },
    Failed { restore_enabled_ids: Vec<String> },
}

impl ManualConnectOutcome {
    pub(super) fn changed(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ManualConnectTracker {
    state: ManualConnectState,
    target_id: Option<String>,
    target_ssid: Option<String>,
    started_at: Option<i64>,
    deadline_ms: Option<i64>,
    restore_enabled_ids: Vec<String>,
    reason: ManualConnectReason,
}

impl Default for ManualConnectTracker {
    fn default() -> Self {
        Self {
            state: ManualConnectState::Idle,
            target_id: None,
            target_ssid: None,
            started_at: None,
            deadline_ms: None,
            restore_enabled_ids: Vec::new(),
            reason: ManualConnectReason::None,
        }
    }
}

impl ManualConnectTracker {
    pub(super) fn state(&self) -> ManualConnectState {
        self.state
    }

    pub(super) fn target_ssid(&self) -> Option<&str> {
        self.target_ssid.as_deref()
    }

    #[cfg(test)]
    pub(super) fn target_id(&self) -> Option<&str> {
        self.target_id.as_deref()
    }

    pub(super) fn started_at(&self) -> Option<i64> {
        self.started_at
    }

    pub(super) fn reason(&self) -> ManualConnectReason {
        self.reason
    }

    pub(super) fn start(
        &mut self,
        target_id: String,
        target_ssid: String,
        started_at: i64,
        restore_enabled_ids: Vec<String>,
    ) {
        self.state = ManualConnectState::Connecting;
        self.target_id = Some(target_id);
        self.target_ssid = Some(target_ssid);
        self.started_at = Some(started_at);
        self.deadline_ms = Some(started_at.saturating_add(DEFAULT_MANUAL_CONNECT_TIMEOUT_MS));
        self.restore_enabled_ids = restore_enabled_ids;
        self.reason = ManualConnectReason::None;
    }

    pub(super) fn observe_read_state(
        &mut self,
        state: &WifiReadState,
        now_ms: i64,
    ) -> ManualConnectOutcome {
        if self.state != ManualConnectState::Connecting {
            return ManualConnectOutcome::None;
        }

        if state.connection_state == WifiConnectionState::Connected
            && state.network_id.as_deref() == self.target_id.as_deref()
        {
            let restore_enabled_ids = self.take_restore_enabled_ids();
            self.clear();
            return ManualConnectOutcome::Succeeded {
                restore_enabled_ids,
            };
        }

        if self
            .deadline_ms
            .is_some_and(|deadline_ms| now_ms >= deadline_ms)
        {
            return self.mark_failed(ManualConnectReason::Timeout);
        }

        ManualConnectOutcome::None
    }

    pub(super) fn observe_wpa_event(&mut self, message: &str) -> ManualConnectOutcome {
        if self.state != ManualConnectState::Connecting {
            return ManualConnectOutcome::None;
        }
        if !message.contains("CTRL-EVENT-SSID-TEMP-DISABLED") {
            return ManualConnectOutcome::None;
        }
        if !self.event_matches_target(message) {
            return ManualConnectOutcome::None;
        }

        self.mark_failed(ManualConnectReason::AuthFailed)
    }

    pub(super) fn observe_backend_error(&mut self) -> ManualConnectOutcome {
        if self.state != ManualConnectState::Connecting {
            return ManualConnectOutcome::None;
        }

        self.mark_failed(ManualConnectReason::BackendError)
    }

    fn mark_failed(&mut self, reason: ManualConnectReason) -> ManualConnectOutcome {
        self.state = ManualConnectState::Failed;
        self.reason = reason;
        ManualConnectOutcome::Failed {
            restore_enabled_ids: self.take_restore_enabled_ids(),
        }
    }

    fn clear(&mut self) {
        self.state = ManualConnectState::Idle;
        self.target_id = None;
        self.target_ssid = None;
        self.started_at = None;
        self.deadline_ms = None;
        self.restore_enabled_ids = Vec::new();
        self.reason = ManualConnectReason::None;
    }

    fn take_restore_enabled_ids(&mut self) -> Vec<String> {
        std::mem::take(&mut self.restore_enabled_ids)
    }

    fn event_matches_target(&self, message: &str) -> bool {
        if let Some(event_id) = event_field(message, "id") {
            return Some(event_id.as_str()) == self.target_id.as_deref();
        }

        if let Some(event_ssid) = event_field(message, "ssid") {
            return Some(event_ssid.as_str()) == self.target_ssid.as_deref();
        }

        false
    }
}

fn event_field(message: &str, field: &str) -> Option<String> {
    let needle = format!("{field}=");
    let mut offset = 0;
    let start = loop {
        let found = message[offset..].find(&needle)? + offset;
        let has_field_boundary = found == 0
            || message[..found]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        if has_field_boundary {
            break found + needle.len();
        }
        offset = found + needle.len();
    };
    let value = &message[start..];

    if let Some(quoted) = value.strip_prefix('"') {
        let end = quoted.find('"')?;
        return Some(quoted[..end].to_string());
    }

    let end = value.find(char::is_whitespace).unwrap_or(value.len());
    Some(value[..end].to_string()).filter(|text| !text.is_empty())
}
