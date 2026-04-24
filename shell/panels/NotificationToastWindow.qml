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

    screen: screenModel
    visible: root.isMainScreen && NotificationUiState.toastSurfaceVisible

    anchors.top: true
    anchors.right: true
    anchors.bottom: true

    margins {
        top: root.topOffset
        right: root.rightInset
        bottom: 0
    }

    exclusionMode: ExclusionMode.Ignore
    exclusiveZone: 0
    aboveWindows: true
    focusable: false
    color: "transparent"
    implicitWidth: Theme.notificationToastColumnWidth

    NotificationToastColumn {
        anchors.fill: parent
    }
}
