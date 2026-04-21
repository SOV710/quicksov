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
        toml::Value::Table(tbl) => Value::Object(
            tbl.iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect(),
        ),
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

/// Humanize desktop/app identifiers such as `org.wezfurlong.wezterm`.
pub fn prettify_app_id(app_id: &str) -> String {
    let mut base = app_id.trim().trim_end_matches(".desktop");
    if let Some(last) = base.rsplit('.').next() {
        if base.contains('.') {
            base = last;
        }
    }

    prettify_app_id_base(base)
}

/// Humanize a free-form label if it looks like an app id or process name.
pub fn prettify_label(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed.contains(' ') || trimmed.chars().any(|ch| ch.is_uppercase()) {
        return trimmed.to_string();
    }

    prettify_app_id(trimmed)
}

fn prettify_app_id_base(base: &str) -> String {
    let mut normalized = base;

    for suffix in ["-bin", "-stable", "-git", "_bin", "_stable", "_git"] {
        if let Some(stripped) = normalized.strip_suffix(suffix) {
            normalized = stripped;
            break;
        }
    }

    let words: Vec<String> = normalized
        .split(['.', '-', '_'])
        .filter(|segment| !segment.is_empty())
        .map(prettify_word)
        .collect();

    if words.is_empty() {
        base.to_string()
    } else {
        words.join(" ")
    }
}

fn prettify_word(word: &str) -> String {
    let lower = word.to_ascii_lowercase();
    match lower.as_str() {
        "ghostty" => "Ghostty".to_string(),
        "wezterm" => "WezTerm".to_string(),
        "vivaldi" => "Vivaldi".to_string(),
        "firefox" => "Firefox".to_string(),
        "thunderbird" => "Thunderbird".to_string(),
        "emacs" => "GNU Emacs".to_string(),
        "nvim" => "Neovim".to_string(),
        _ => {
            let mut chars = lower.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut result = first.to_uppercase().collect::<String>();
            result.push_str(chars.as_str());
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{prettify_app_id, prettify_label};

    #[test]
    fn prettify_app_id_handles_known_aliases_and_suffixes() {
        assert_eq!(prettify_app_id("org.wezfurlong.wezterm"), "WezTerm");
        assert_eq!(prettify_app_id("ghostty-bin.desktop"), "Ghostty");
    }

    #[test]
    fn prettify_label_preserves_already_human_text() {
        assert_eq!(prettify_label("Visual Studio Code"), "Visual Studio Code");
        assert_eq!(prettify_label("Firefox"), "Firefox");
    }
}
