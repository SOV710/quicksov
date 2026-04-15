// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Effects

// Renders a monochrome SVG icon (lucide/phosphor) tinted to `color`.
// iconPath: relative to the icons/ root, e.g. "lucide/wifi.svg"
Item {
    id: root

    property string iconPath: ""
    property color  color: "white"
    property int    size: 16

    width:  root.size
    height: root.size

    // Off-screen source — rendered but not displayed
    Image {
        id: _src
        anchors.fill: parent
        source: root.iconPath
                ? Qt.resolvedUrl("../icons/" + root.iconPath)
                : ""
        fillMode: Image.PreserveAspectFit
        visible: false
        smooth: true
    }

    // Colored rectangle masked by the SVG's alpha channel
    Rectangle {
        anchors.fill: parent
        color: root.color
        visible: _src.status === Image.Ready
        layer.enabled: true
        layer.effect: MultiEffect {
            maskEnabled: true
            maskSource: _src
            maskThresholdMin: 0.0
            maskSpreadAtMin: 0.0
        }
    }

    // Fallback border when icon fails to load
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.color: root.color
        border.width: 1
        radius: 2
        visible: _src.status === Image.Error || root.iconPath === ""
    }
}
