// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use serde::Deserialize;

use crate::config::paths::DAEMON_SOCKET_RAW;

/// Top-level daemon configuration, sourced from `daemon.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
    /// Screen-to-role mapping; parsed but not acted on until a Niri service is active.
    #[serde(default)]
    pub screens: ScreensConfig,
    #[serde(default)]
    pub power: PowerConfig,
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
    DAEMON_SOCKET_RAW.to_string()
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

/// Power action enablement exposed to the shell UI.
#[derive(Debug, Deserialize)]
pub struct PowerConfig {
    #[serde(default = "default_power_action_enabled")]
    pub lock: bool,
    #[serde(default = "default_power_action_enabled")]
    pub suspend: bool,
    #[serde(default = "default_power_action_enabled")]
    pub logout: bool,
    #[serde(default = "default_power_action_enabled")]
    pub reboot: bool,
    #[serde(default = "default_power_action_enabled")]
    pub shutdown: bool,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            lock: default_power_action_enabled(),
            suspend: default_power_action_enabled(),
            logout: default_power_action_enabled(),
            reboot: default_power_action_enabled(),
            shutdown: default_power_action_enabled(),
        }
    }
}

fn default_power_action_enabled() -> bool {
    true
}

/// Which services to enable and their per-service configuration.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Default)]
pub struct ServicesConfig {
    /// Ordered list of topic names that should be started at daemon boot.
    #[serde(default)]
    pub enabled: Vec<String>,
    pub weather: Option<WeatherConfig>,
    pub wallpaper: Option<WallpaperConfig>,
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

/// Configuration for the `wallpaper` service.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct WallpaperConfig {
    pub directory: Option<String>,
    pub transition: Option<String>,
    pub transition_duration_ms: Option<u64>,
    pub renderer: Option<String>,
    pub decode_backend_order: Option<Vec<String>>,
    pub decode_device_policy: Option<String>,
    pub render_device_policy: Option<String>,
    pub allow_cross_gpu: Option<bool>,
    pub present_backend: Option<String>,
    pub present_mode: Option<String>,
    pub vsync: Option<bool>,
    pub video_audio: Option<bool>,
    #[serde(default)]
    pub sources: HashMap<String, WallpaperSourceConfig>,
    #[serde(default)]
    pub views: HashMap<String, WallpaperViewConfig>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct WallpaperSourceConfig {
    pub path: String,
    pub kind: Option<String>,
    #[serde(rename = "loop")]
    pub loop_enabled: Option<bool>,
    pub mute: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct WallpaperViewConfig {
    pub source: String,
    pub fit: Option<String>,
    pub crop: Option<WallpaperCropConfig>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct WallpaperCropConfig {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
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
