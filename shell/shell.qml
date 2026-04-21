// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import Quickshell
import "./bars"
import "./desktop"
import "./services"

Scope {
    readonly property bool _themeServiceBoot: ThemeService.ready || ThemeService.connected

    MainBar {}
    AuxBar {}
    PowerDock {}
}
