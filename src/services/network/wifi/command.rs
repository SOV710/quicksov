// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::Value;

use crate::bus::ServiceError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConnectRequest {
    pub(super) ssid: String,
    pub(super) psk: Option<String>,
    pub(super) save: bool,
}

impl ConnectRequest {
    pub(super) fn from_payload(payload: &Value) -> Result<Self, ServiceError> {
        Ok(Self {
            ssid: extract_str(payload, "ssid").ok_or_else(|| ServiceError::ActionPayload {
                msg: "missing 'ssid' field".to_string(),
            })?,
            psk: extract_str(payload, "psk"),
            save: extract_bool(payload, "save").unwrap_or(false),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ForgetRequest {
    pub(super) ssid: String,
}

impl ForgetRequest {
    pub(super) fn from_payload(payload: &Value) -> Result<Self, ServiceError> {
        Ok(Self {
            ssid: extract_str(payload, "ssid").ok_or_else(|| ServiceError::ActionPayload {
                msg: "missing 'ssid' field".to_string(),
            })?,
        })
    }
}

pub(super) fn enabled_from_payload(payload: &Value) -> Result<bool, ServiceError> {
    extract_bool(payload, "enabled").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'enabled' bool field".to_string(),
    })
}

pub(super) fn escape_wpa_string(value: &str) -> Result<String, ServiceError> {
    if value.contains(['\n', '\r', '\0']) {
        return Err(ServiceError::ActionPayload {
            msg: "wifi strings must not contain control newlines or NUL".to_string(),
        });
    }

    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(ch),
        }
    }
    Ok(escaped)
}

fn extract_str(v: &Value, key: &str) -> Option<String> {
    v.as_object()?.get(key)?.as_str().map(ToString::to_string)
}

fn extract_bool(v: &Value, key: &str) -> Option<bool> {
    v.as_object()?.get(key)?.as_bool()
}
