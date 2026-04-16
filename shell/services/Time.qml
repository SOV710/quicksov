// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import Quickshell

/// Single resident time source.
/// All clock displays bind to `Time.now` instead of owning private timers.
/// The internal timer self-corrects to second boundaries so `now` is always
/// within a few ms of the true second tick.
Singleton {
    id: root

    /// Current time, updated on each second boundary.
    property var now: new Date()

    Timer {
        id: ticker
        running: true
        repeat: false
        // Fire at the next second boundary
        interval: 1000 - (new Date()).getMilliseconds()
        onTriggered: {
            root.now = new Date();
            // Re-arm: recalculate offset each tick to stay accurate over long uptime
            var drift = root.now.getMilliseconds();
            ticker.interval = drift > 50 ? (1000 - drift + 10) : 1000;
            ticker.repeat = false;
            ticker.restart();
        }
    }
}
