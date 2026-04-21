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
        (default_config_text(), true)
    };

    let mut config: Config = toml::from_str(&raw)?;
    expand_config_paths(&mut config)?;

    Ok((config, used_defaults))
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn default_config_text() -> String {
    format!(
        "[daemon]\nlog_level = \"info\"\nsocket_path = \"{}\"\n\n[screens]\n\n[power]\nlock = true\nsuspend = true\nlogout = true\nreboot = true\nshutdown = true\n\n[services]\nenabled = []\n",
        paths::DAEMON_SOCKET_RAW
    )
}

fn config_file_path() -> Result<PathBuf, ConfigError> {
    paths::daemon_config_path().ok_or(ConfigError::NoHomeDir)
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
        for source in wallpaper.sources.values_mut() {
            source.path = paths::expand_env_vars(&source.path)?;
        }
    }

    Ok(())
}
