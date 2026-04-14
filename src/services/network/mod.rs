// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Network services — `net.link` (rtnetlink) and `net.wifi` (wpa_supplicant).

pub mod link;
pub mod wifi;

pub use link::spawn_link;
pub use wifi::spawn_wifi;
