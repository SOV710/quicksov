// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Deserialize;

/// Top-level daemon configuration, sourced from `daemon.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
    /// Screen-to-role mapping; parsed but not acted on until a Niri service is active.
    #[serde(default)]
    pub screens: ScreensConfig,
    #[serde(default)]
    pub services: ServicesConfig,
}

/// Core daemon runtime parameters.
#[derive(Debug, Deserialize)]
pub struct DaemonConfig {
    /// `tracing` filter string, e.g. `"info"` or `"debug,quicksov=trace"`.
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Path of the Unix Domain Socket; may contain `$XDG_RUNTIME_DIR`.
    #[serde(default = "default_socket_path_raw")]
    pub socket_path: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            socket_path: default_socket_path_raw(),
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_socket_path_raw() -> String {
    "$XDG_RUNTIME_DIR/quicksov/daemon.sock".to_string()
}

/// Screen-to-role mapping configuration.
#[derive(Debug, Deserialize, Default)]
pub struct ScreensConfig {
    /// Simple single-field alternative: name the main screen directly.
    /// When set, all other screens are implicitly treated as "aux".
    /// Takes priority over `mapping` when both are present.
    pub main_screen: Option<String>,
    #[serde(default)]
    pub mapping: Vec<ScreenMapping>,
}

/// Maps a DRM connector name (e.g. `"DP-1"`) to a logical role (`"main"`, `"aux"`).
#[derive(Debug, Deserialize)]
pub struct ScreenMapping {
    pub match_name: String,
    pub role: String,
}

/// Which services to enable and their per-service configuration.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Default)]
pub struct ServicesConfig {
    /// Ordered list of topic names that should be started at daemon boot.
    #[serde(default)]
    pub enabled: Vec<String>,
    pub weather: Option<WeatherConfig>,
    pub network: Option<NetworkConfig>,
    pub audio: Option<AudioConfig>,
    pub niri: Option<NiriConfig>,
}

/// Configuration for the `weather` service (Open-Meteo backend).
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct WeatherConfig {
    pub backend: Option<String>,
    pub location_mode: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub location_name: Option<String>,
    pub poll_interval_sec: Option<u64>,
    pub units: Option<String>,
}

/// Configuration for the `net.link` / `net.wifi` services.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct NetworkConfig {
    pub wifi_backend: Option<String>,
    pub wpa_ctrl_path: Option<String>,
    pub interfaces: Option<Vec<String>>,
}

/// Configuration for the `audio` service (PipeWire backend).
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AudioConfig {
    pub backend: Option<String>,
}

/// Configuration for the `niri` service.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct NiriConfig {
    /// Path to the Niri IPC socket; may contain `$NIRI_SOCKET`.
    pub socket: Option<String>,
}
