// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use nix::unistd::getuid;
use thiserror::Error;

pub const APP_DIR_NAME: &str = "quicksov";
pub const DAEMON_SOCKET_RAW: &str = "$XDG_RUNTIME_DIR/quicksov/daemon.sock";
pub const QSOSYSD_SOCKET_ADDR_RAW: &str = "\0quicksov.qsosysd";
pub const QSOSYSD_SOCKET_ADDR_DISPLAY: &str = "@quicksov.qsosysd";
pub const DAEMON_CONFIG_FILE_NAME: &str = "daemon.toml";
pub const DESIGN_TOKENS_FILE_NAME: &str = "design-tokens.toml";
pub const WEATHER_CACHE_FILE_NAME: &str = "current.json";
pub const NIRI_SOCKET_RELATIVE_PATH: &str = "niri/socket";

/// Errors encountered while expanding environment variables in path strings.
#[derive(Debug, Error)]
pub enum PathsError {
    #[error("environment variable ${0} is not set (required for path expansion)")]
    EnvVarNotSet(String),
}

pub fn runtime_dir() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(format!("/run/user/{}", getuid().as_raw())))
}

pub fn config_dir() -> Option<PathBuf> {
    if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg_config).join(APP_DIR_NAME));
    }
    dirs::home_dir().map(|home| home.join(".config").join(APP_DIR_NAME))
}

pub fn daemon_config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(DAEMON_CONFIG_FILE_NAME))
}

pub fn design_tokens_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(DESIGN_TOKENS_FILE_NAME))
}

pub fn default_wallpaper_directory() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("wallpapers"))
}

pub fn weather_cache_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join(".cache")
            .join(APP_DIR_NAME)
            .join("weather")
            .join(WEATHER_CACHE_FILE_NAME)
    })
}

pub fn default_session_bus_address() -> String {
    format!("unix:path={}", runtime_dir().join("bus").display())
}

pub fn default_niri_socket_path() -> PathBuf {
    runtime_dir().join(NIRI_SOCKET_RELATIVE_PATH)
}

/// Expand `$VAR` references in `s` using the process environment.
///
/// Scans for `$` followed by alphanumeric-and-underscore characters and replaces
/// each occurrence with `std::env::var`. Returns an error if any referenced
/// variable is not set in the environment.
pub fn expand_env_vars(s: &str) -> Result<String, PathsError> {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let var_name = &s[start..i];
            if var_name.is_empty() {
                result.push('$');
            } else {
                let val = std::env::var(var_name)
                    .map_err(|_| PathsError::EnvVarNotSet(var_name.to_string()))?;
                result.push_str(&val);
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_plain_string_unchanged() {
        assert_eq!(
            expand_env_vars("/run/user/1000/foo.sock").unwrap(),
            "/run/user/1000/foo.sock"
        );
    }

    #[test]
    fn expand_missing_var_errors() {
        std::env::remove_var("__QUICKSOV_NONEXISTENT_VAR__");
        let err = expand_env_vars("$__QUICKSOV_NONEXISTENT_VAR__/foo");
        assert!(err.is_err());
    }
}
