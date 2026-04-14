// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared conversion and time helpers used across services.

use serde_json::{Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// Convert a [`toml::Value`] to a [`serde_json::Value`].
pub fn toml_to_json(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(i) => Value::Number((*i).into()),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(arr) => Value::Array(arr.iter().map(toml_to_json).collect()),
        toml::Value::Table(tbl) => {
            Value::Object(tbl.iter().map(|(k, v)| (k.clone(), toml_to_json(v))).collect())
        }
    }
}

/// Build a JSON object from an iterator of `(key, value)` pairs.
pub fn json_map(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<Map<String, Value>>(),
    )
}

/// Returns `true` if `v` is a JSON null or an empty object `{}`.
///
/// Clients that omit a payload field send `null`; clients that send `{}` send
/// an empty object.  Both are accepted as "empty object" for action validation.
pub fn is_empty_object(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Object(m) => m.is_empty(),
        _ => false,
    }
}

/// Current Unix timestamp in milliseconds.
pub fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Current Unix timestamp in seconds.
pub fn unix_now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
