// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io;

use crate::bus::ServiceError;

#[derive(Debug, thiserror::Error)]
pub(super) enum WifiError {
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

pub(super) fn service_error_from_wifi_error(err: WifiError) -> ServiceError {
    ServiceError::Internal {
        msg: err.to_string(),
    }
}
