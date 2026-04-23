// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Item {
    id: root

    property var shellModel: null
    property color fillColor: Theme.barShellFill
    property color strokeColor: Theme.barShellBorder

    PanelShellLayer {
        anchors.fill: parent
        primitives: root.shellModel ? root.shellModel.outer : null
        color: root.strokeColor
    }

    PanelShellLayer {
        anchors.fill: parent
        primitives: root.shellModel ? root.shellModel.inner : null
        color: root.fillColor
    }
}
