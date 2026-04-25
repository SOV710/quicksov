// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import ".."
import "../overlays"
import "../services"

PanelWindow {
    id: root

    required property var screenModel

    readonly property bool isMainScreen: Meta.ready
                                          && Meta.hasScreenRoles
                                          && Meta.screenRoles[screenModel.name] === "main"
    readonly property real rightInset: Theme.barOuterMargin
                                       + Theme.statusDockLowerRadius
    readonly property real topOffset: Theme.barOuterMargin
                                      + Theme.barHeight
                                      + Theme.statusDockShoulderDepth
                                      - Theme.statusDockSeamOverlap
    readonly property real availableHeight: screen && screen.height > 0
                                            ? Math.max(0, screen.height - root.topOffset)
                                            : 0

    screen: screenModel
    visible: root.isMainScreen

    anchors.top: true
    anchors.right: true

    margins {
        top: root.topOffset
        right: root.rightInset
    }

    exclusionMode: ExclusionMode.Ignore
    exclusiveZone: 0
    aboveWindows: true
    focusable: false
    color: "transparent"
    implicitWidth: Theme.notificationToastColumnWidth
    implicitHeight: toastColumn.implicitHeight
    mask: Region {
        item: toastColumn.toastBoundsItem
    }

    NotificationToastColumn {
        id: toastColumn

        width: parent ? parent.width : Theme.notificationToastColumnWidth
        height: implicitHeight
        availableHeight: root.availableHeight
    }
}
