// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import "./services"
import "./bars"

Item {
    id: root
    required property var modelData

    property string screenRole: Meta.screenRoles[modelData.name] ?? ""

    Loader {
        active: root.screenRole === "main"
        sourceComponent: MainBar { screen: root.modelData }
    }

    Loader {
        active: root.screenRole === "aux"
        sourceComponent: AuxBar { screen: root.modelData }
    }
}
