// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::bus::ServiceHandle;
use crate::config::Config;

/// Start all configured services and return a handle map.
///
/// In Phase 1, no services are implemented; this always returns an empty map.
/// Future phases will consult `cfg.services.enabled` and spawn the appropriate
/// service tasks here.
pub async fn start_services(cfg: &Config) -> HashMap<String, ServiceHandle> {
    let _ = cfg;
    HashMap::new()
}
