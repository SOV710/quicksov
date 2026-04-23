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

        Rectangle {
            id: leftEllipse
            readonly property real baseRadius: Math.max(leftShoulder.width, leftShoulder.height)
            x: -baseRadius
            y: leftShoulder.height - baseRadius
            width: baseRadius * 2
            height: baseRadius * 2
            radius: baseRadius
            color: root.color
            antialiasing: true

            transform: Scale {
                origin.x: leftEllipse.baseRadius
                origin.y: leftEllipse.baseRadius
                xScale: leftEllipse.baseRadius > 0
                        ? leftShoulder.width / leftEllipse.baseRadius
                        : 1
                yScale: leftEllipse.baseRadius > 0
                        ? leftShoulder.height / leftEllipse.baseRadius
                        : 1
            }
        }
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

        Rectangle {
            id: rightEllipse
            readonly property real baseRadius: Math.max(rightShoulder.width, rightShoulder.height)
            x: rightShoulder.width - baseRadius
            y: rightShoulder.height - baseRadius
            width: baseRadius * 2
            height: baseRadius * 2
            radius: baseRadius
            color: root.color
            antialiasing: true

            transform: Scale {
                origin.x: rightEllipse.baseRadius
                origin.y: rightEllipse.baseRadius
                xScale: rightEllipse.baseRadius > 0
                        ? rightShoulder.width / rightEllipse.baseRadius
                        : 1
                yScale: rightEllipse.baseRadius > 0
                        ? rightShoulder.height / rightEllipse.baseRadius
                        : 1
            }
        }
    }
}
