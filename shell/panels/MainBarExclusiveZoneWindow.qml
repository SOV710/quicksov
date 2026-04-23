// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import ".."

PanelWindow {
    id: root

    required property var screenModel
    screen: screenModel
    visible: true

    anchors.top: true
    anchors.left: true
    anchors.right: true

    margins {
        top: Theme.barOuterMargin
        left: Theme.barOuterMargin
        right: Theme.barOuterMargin
    }

    implicitHeight: Theme.barHeight
    exclusiveZone: Theme.barHeight
    color: "transparent"
    focusable: false

    Item {
        id: reserveBar
        anchors.fill: parent
        visible: false
    }

    mask: Region {
        item: reserveBar
        radius: Theme.barRadius
    }
}
