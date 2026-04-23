// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Item {
    id: root

    property var primitives: null
    property color color: "white"
    property real seamOverlap: 1

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
        x: root.primitives ? root.primitives.neckX : 0
        y: root.primitives ? root.primitives.neckY : 0
        width: root.primitives ? root.primitives.neckWidth : 0
        height: root.primitives ? root.primitives.neckHeight : 0
        color: root.color
        visible: root.primitives
                 && root.primitives.panelActive
                 && width > 0
                 && height > 0
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

    Item {
        id: leftShoulder
        x: root.primitives ? root.primitives.leftShoulderClipX : 0
        y: root.primitives ? root.primitives.leftShoulderClipY : 0
        width: root.primitives ? root.primitives.leftShoulderClipWidth : 0
        height: root.primitives ? root.primitives.leftShoulderClipHeight : 0
        clip: true
        visible: root.primitives
                 && root.primitives.panelActive
                 && width > 0
                 && height > 0

        PanelShoulderFill {
            anchors.fill: parent
            color: root.color
            direction: "left"
        }
    }

    Rectangle {
        x: leftShoulder.x
        y: leftShoulder.y - Math.min(root.seamOverlap, leftShoulder.height)
        width: leftShoulder.width
        height: Math.min(root.seamOverlap, leftShoulder.height)
        color: root.color
        antialiasing: false
        visible: leftShoulder.visible
    }

    Rectangle {
        x: leftShoulder.x + leftShoulder.width
        y: leftShoulder.y
        width: root.seamOverlap
        height: leftShoulder.height
        color: root.color
        antialiasing: false
        visible: leftShoulder.visible
    }

    Item {
        id: rightShoulder
        x: root.primitives ? root.primitives.rightShoulderClipX : 0
        y: root.primitives ? root.primitives.rightShoulderClipY : 0
        width: root.primitives ? root.primitives.rightShoulderClipWidth : 0
        height: root.primitives ? root.primitives.rightShoulderClipHeight : 0
        clip: true
        visible: root.primitives
                 && root.primitives.panelActive
                 && width > 0
                 && height > 0

        PanelShoulderFill {
            anchors.fill: parent
            color: root.color
            direction: "right"
        }
    }

    Rectangle {
        x: rightShoulder.x
        y: rightShoulder.y - Math.min(root.seamOverlap, rightShoulder.height)
        width: rightShoulder.width
        height: Math.min(root.seamOverlap, rightShoulder.height)
        color: root.color
        antialiasing: false
        visible: rightShoulder.visible
    }

    Rectangle {
        x: rightShoulder.x - root.seamOverlap
        y: rightShoulder.y
        width: root.seamOverlap
        height: rightShoulder.height
        color: root.color
        antialiasing: false
        visible: rightShoulder.visible
    }
}
