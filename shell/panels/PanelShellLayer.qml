// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
Item {
    id: root

    property var primitives: null
    property color color: "white"

    Rectangle {
        x: root.primitives ? root.primitives.barX : 0
        y: root.primitives ? root.primitives.barY : 0
        width: root.primitives ? root.primitives.barWidth : 0
        height: root.primitives ? root.primitives.barHeight : 0
        radius: root.primitives ? root.primitives.barRadius : 0
        color: root.color
        antialiasing: true
        visible: width > 0 && height > 0
    }

    Rectangle {
        x: root.primitives ? root.primitives.bodyX : 0
        y: root.primitives ? root.primitives.bodyY : 0
        width: root.primitives ? root.primitives.bodyWidth : 0
        height: root.primitives ? root.primitives.bodyHeight : 0
        color: root.color
        topLeftRadius: 0
        topRightRadius: 0
        bottomLeftRadius: root.primitives ? root.primitives.bodyRadius : 0
        bottomRightRadius: root.primitives ? root.primitives.bodyRadius : 0
        antialiasing: true
        visible: root.primitives
                 && root.primitives.panelActive
                 && width > 0
                 && height > 0
    }

}
