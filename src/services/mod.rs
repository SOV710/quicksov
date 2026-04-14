// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::time::Instant;

use crate::bus::ServiceHandle;
use crate::config::Config;

pub mod meta;

/// Start all configured services and return a handle map keyed by topic name.
///
/// Only topics listed in `cfg.services.enabled` are started.  `started_at` is
/// the process-level start instant, forwarded to services that expose uptime
/// information (currently `meta`).
pub async fn start_services(cfg: &Config, started_at: Instant) -> HashMap<String, ServiceHandle> {
    let mut map: HashMap<String, ServiceHandle> = HashMap::new();

    for topic in &cfg.services.enabled {
        match topic.as_str() {
            "meta" => {
                let handle = meta::spawn(started_at, cfg.services.enabled.clone());
                map.insert("meta".to_string(), handle);
            }
            other => {
                tracing::warn!(topic = %other, "unknown service topic in enabled list; skipping");
            }
        }
    }

    map
}
