// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::time::Instant;

use crate::bus::ServiceHandle;
use crate::config::Config;

pub mod audio;
pub mod battery;
pub mod bluetooth;
pub mod meta;
pub mod mpris;
pub mod network;
pub mod niri;
pub mod notification;
pub mod theme;
pub mod wallpaper;
pub mod weather;

/// Start all configured services and return a handle map keyed by topic name.
///
/// Only topics listed in `cfg.services.enabled` are started.  `started_at` is
/// the process-level start instant, forwarded to services that expose uptime
/// information (currently `meta`).
pub async fn start_services(cfg: &Config, started_at: Instant) -> HashMap<String, ServiceHandle> {
    let mut map: HashMap<String, ServiceHandle> = HashMap::new();

    for topic in &cfg.services.enabled {
        match topic.as_str() {
            "battery" => {
                map.insert("battery".into(), battery::spawn(cfg));
            }
            "net.link" => {
                map.insert("net.link".into(), network::spawn_link(cfg));
            }
            "net.wifi" => {
                map.insert("net.wifi".into(), network::spawn_wifi(cfg));
            }
            "bluetooth" => {
                map.insert("bluetooth".into(), bluetooth::spawn(cfg));
            }
            "audio" => {
                map.insert("audio".into(), audio::spawn(cfg));
            }
            "mpris" => {
                map.insert("mpris".into(), mpris::spawn(cfg));
            }
            "notification" => {
                map.insert("notification".into(), notification::spawn(cfg));
            }
            "niri" => {
                map.insert("niri".into(), niri::spawn(cfg));
            }
            "weather" => {
                map.insert("weather".into(), weather::spawn(cfg));
            }
            "wallpaper" => {
                map.insert("wallpaper".into(), wallpaper::spawn(cfg));
            }
            "theme" => {
                map.insert("theme".into(), theme::spawn(cfg));
            }
            "meta" => {} // registered after the loop
            other => {
                tracing::warn!(topic = %other, "unknown service topic; skipping");
            }
        }
    }

    // meta is always last so it can report all running services
    if cfg.services.enabled.iter().any(|t| t == "meta") {
        let running: Vec<String> = map
            .keys()
            .cloned()
            .chain(std::iter::once("meta".to_string()))
            .collect();

        // Build screen roles: `main_screen` takes priority over the `mapping` list.
        let screens_roles: std::collections::HashMap<String, String> =
            if let Some(main) = cfg.screens.main_screen.as_deref() {
                let mut m = std::collections::HashMap::new();
                m.insert(main.to_string(), "main".to_string());
                m
            } else {
                cfg.screens
                    .mapping
                    .iter()
                    .map(|m| (m.match_name.clone(), m.role.clone()))
                    .collect()
            };
        let power_actions: std::collections::HashMap<String, bool> = [
            ("lock".to_string(), cfg.power.lock),
            ("suspend".to_string(), cfg.power.suspend),
            ("logout".to_string(), cfg.power.logout),
            ("reboot".to_string(), cfg.power.reboot),
            ("shutdown".to_string(), cfg.power.shutdown),
        ]
        .into_iter()
        .collect();
        map.insert(
            "meta".into(),
            meta::spawn(started_at, running, screens_roles, power_actions),
        );
    }

    map
}
