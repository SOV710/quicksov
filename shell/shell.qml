// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import "."
import "./ipc"
import "./services"
import "./bars"

ShellRoot {
    // Ensure singletons are instantiated.
    Client { id: _client }
    ThemeService { id: _themeService }
    Meta { id: _meta }

    Scope {
        Variants {
            model: Quickshell.screens
            delegate: ScreenDelegate {
                required property var modelData
            }
        }
    }
}
