// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelperRequest {
    pub action: String,
    #[serde(default)]
    pub profile: String,
}

impl HelperRequest {
    pub const SET_PLATFORM_PROFILE_ACTION: &'static str = "set_platform_profile";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HelperErrorKind {
    Unsupported,
    PermissionDenied,
    BackendUnavailable,
    WriteFailed,
    InvalidRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HelperResponse {
    Ok {
        profile: String,
        raw_profile: String,
    },
    Error {
        kind: HelperErrorKind,
        message: String,
    },
}
