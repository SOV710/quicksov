// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared conversion and time helpers used across services.

use rmpv::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Convert a [`serde_json::Value`] to an [`rmpv::Value`].
pub fn json_to_rmpv(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else if let Some(f) = n.as_f64() {
                Value::from(f)
            } else {
                Value::Nil
            }
        }
        serde_json::Value::String(s) => Value::from(s.as_str()),
        serde_json::Value::Array(arr) => Value::Array(arr.iter().map(json_to_rmpv).collect()),
        serde_json::Value::Object(obj) => {
            let pairs: Vec<(Value, Value)> = obj
                .iter()
                .map(|(k, v)| (Value::from(k.as_str()), json_to_rmpv(v)))
                .collect();
            Value::Map(pairs)
        }
    }
}

/// Convert a [`toml::Value`] to an [`rmpv::Value`].
pub fn toml_to_rmpv(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::from(s.as_str()),
        toml::Value::Integer(i) => Value::from(*i),
        toml::Value::Float(f) => Value::from(*f),
        toml::Value::Boolean(b) => Value::Boolean(*b),
        toml::Value::Datetime(dt) => Value::from(dt.to_string().as_str()),
        toml::Value::Array(arr) => Value::Array(arr.iter().map(toml_to_rmpv).collect()),
        toml::Value::Table(tbl) => {
            let pairs: Vec<(Value, Value)> = tbl
                .iter()
                .map(|(k, v)| (Value::from(k.as_str()), toml_to_rmpv(v)))
                .collect();
            Value::Map(pairs)
        }
    }
}

/// Build an rmpv map from an iterator of `(key, value)` pairs.
pub fn rmpv_map(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Map(
        pairs
            .into_iter()
            .map(|(k, v)| (Value::from(k), v))
            .collect(),
    )
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
