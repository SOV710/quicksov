// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{SystemTime, UNIX_EPOCH};

use super::error::WifiError;
use super::model::{ScanRequestOutcome, WifiScanState};

#[derive(Clone, Debug, Default)]
pub(super) struct ScanTracker {
    state: WifiScanState,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    last_error: Option<String>,
    stop_requested: bool,
}

impl ScanTracker {
    pub(super) fn state(&self) -> WifiScanState {
        self.state
    }

    pub(super) fn started_at(&self) -> Option<i64> {
        self.started_at
    }

    pub(super) fn finished_at(&self) -> Option<i64> {
        self.finished_at
    }

    pub(super) fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    #[cfg(test)]
    pub(super) fn stop_requested(&self) -> bool {
        self.stop_requested
    }

    pub(super) fn is_idle(&self) -> bool {
        self.state == WifiScanState::Idle
    }

    pub(super) fn clear_activity(&mut self) {
        self.state = WifiScanState::Idle;
        self.stop_requested = false;
    }

    pub(super) fn finish_scan_start(
        &mut self,
        outcome: Result<ScanRequestOutcome, WifiError>,
    ) -> Result<(), WifiError> {
        match outcome {
            Ok(ScanRequestOutcome::Started) => self.mark_starting(),
            Ok(ScanRequestOutcome::Busy) => {
                self.mark_running();
                self.stop_requested = false;
            }
            Err(err) => {
                self.mark_start_failed(err.to_string());
                return Err(err);
            }
        }

        Ok(())
    }

    pub(super) fn finish_scan_stop(
        &mut self,
        result: Result<(), WifiError>,
    ) -> Result<(), WifiError> {
        match result {
            Ok(()) => {
                self.mark_stop_requested();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    pub(super) fn observe_status_scan(&mut self, status_scan_active: bool) {
        if status_scan_active {
            self.mark_running();
        } else if self.stop_requested && self.state != WifiScanState::Idle {
            self.mark_finished();
        }
    }

    pub(super) fn observe_wpa_event(&mut self, message: &str) -> bool {
        let mut relevant = false;

        if message.contains("CTRL-EVENT-SCAN-STARTED") {
            self.mark_running();
            relevant = true;
        }

        if message.contains("CTRL-EVENT-SCAN-RESULTS") {
            self.mark_finished();
            relevant = true;
        }

        if message.contains("CTRL-EVENT-SCAN-FAILED") {
            if self.stop_requested {
                self.mark_finished();
            } else {
                self.mark_runtime_failed(message.trim().to_string());
            }
            relevant = true;
        }

        relevant
            || message.contains("CTRL-EVENT-CONNECTED")
            || message.contains("CTRL-EVENT-DISCONNECTED")
            || message.contains("CTRL-EVENT-SSID-TEMP-DISABLED")
            || message.contains("CTRL-EVENT-SSID-REENABLED")
    }

    pub(super) fn mark_runtime_failed(&mut self, error: String) {
        self.state = WifiScanState::Idle;
        self.finished_at = Some(unix_ms_now());
        self.last_error = Some(error);
        self.stop_requested = false;
    }

    #[cfg(test)]
    pub(super) fn set_for_test(
        &mut self,
        state: WifiScanState,
        started_at: Option<i64>,
        finished_at: Option<i64>,
        last_error: Option<String>,
        stop_requested: bool,
    ) {
        self.state = state;
        self.started_at = started_at;
        self.finished_at = finished_at;
        self.last_error = last_error;
        self.stop_requested = stop_requested;
    }

    fn mark_starting(&mut self) {
        self.state = WifiScanState::Starting;
        self.started_at = Some(unix_ms_now());
        self.last_error = None;
        self.stop_requested = false;
    }

    fn mark_running(&mut self) {
        self.state = WifiScanState::Running;
        if self.started_at.is_none() {
            self.started_at = Some(unix_ms_now());
        }
    }

    fn mark_finished(&mut self) {
        self.state = WifiScanState::Idle;
        self.finished_at = Some(unix_ms_now());
        self.stop_requested = false;
    }

    fn mark_start_failed(&mut self, error: String) {
        self.state = WifiScanState::Idle;
        self.last_error = Some(error);
        self.stop_requested = false;
    }

    fn mark_stop_requested(&mut self) {
        self.stop_requested = true;
    }
}

fn unix_ms_now() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
