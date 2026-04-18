// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod paths;
pub mod schema;

pub use schema::*;

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur while loading or validating the daemon configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file at {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse daemon.toml: {0}")]
    ParseToml(#[from] toml::de::Error),
    #[error("path expansion error: {0}")]
    PathExpansion(#[from] paths::PathsError),
    #[error("cannot determine home directory; $HOME is unset")]
    NoHomeDir,
}

/// Embedded fallback configuration used when `daemon.toml` is absent.
const DEFAULT_CONFIG: &str = r#"
[daemon]
log_level = "info"
socket_path = "$XDG_RUNTIME_DIR/quicksov/daemon.sock"

[screens]

[power]
lock = true
suspend = true
logout = true
reboot = true
shutdown = true

[services]
enabled = []
"#;

/// Load and return the daemon configuration.
///
/// Returns `(Config, used_defaults)`. `used_defaults` is `true` when the
/// config file was absent and embedded defaults were used; the caller should
/// emit a `warn!` after tracing is initialised.
pub fn load_with_info() -> Result<(Config, bool), ConfigError> {
    let config_path = config_file_path()?;

    let (raw, used_defaults) = if config_path.exists() {
        let text =
            std::fs::read_to_string(&config_path).map_err(|source| ConfigError::ReadFile {
                path: config_path.clone(),
                source,
            })?;
        (text, false)
    } else {
        (DEFAULT_CONFIG.to_string(), true)
    };

    let mut config: Config = toml::from_str(&raw)?;
    expand_config_paths(&mut config)?;

    Ok((config, used_defaults))
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn config_file_path() -> Result<PathBuf, ConfigError> {
    if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg_config)
            .join("quicksov")
            .join("daemon.toml"));
    }
    let home = dirs::home_dir().ok_or(ConfigError::NoHomeDir)?;
    Ok(home.join(".config").join("quicksov").join("daemon.toml"))
}

fn expand_config_paths(config: &mut Config) -> Result<(), ConfigError> {
    config.daemon.socket_path = paths::expand_env_vars(&config.daemon.socket_path)?;

    if let Some(niri) = config.services.niri.as_mut() {
        if let Some(socket) = niri.socket.as_mut() {
            *socket = paths::expand_env_vars(socket)?;
        }
    }

    if let Some(wallpaper) = config.services.wallpaper.as_mut() {
        if let Some(directory) = wallpaper.directory.as_mut() {
            *directory = paths::expand_env_vars(directory)?;
        }
    }

    Ok(())
}
