// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

QtObject {
    id: root

    property Item barItem: null
    property Item coordinateItem: null
    property var geometry: null
    property int borderThickness: 1

    function positive(value) {
        return Math.max(0, value);
    }

    function clampRadius(radius, width, height) {
        return Math.max(0, Math.min(radius, width / 2, height / 2));
    }

    readonly property real barX: {
        if (barItem && coordinateItem) {
            var point = barItem.mapToItem(coordinateItem, 0, 0);
            return point.x;
        }
        return 0;
    }
    readonly property real barY: {
        if (barItem && coordinateItem) {
            var point = barItem.mapToItem(coordinateItem, 0, 0);
            return point.y;
        }
        return 0;
    }
    readonly property real barWidth: barItem ? barItem.width : 0
    readonly property real barHeight: barItem ? barItem.height : 0
    readonly property real barRadius: clampRadius(Theme.barRadius, barWidth, barHeight)

    readonly property bool panelActive: geometry
                                       && geometry.active
                                       && geometry.width > 0
                                       && geometry.shoulderHeight > 0
    readonly property real panelX: panelActive ? geometry.x : 0
    readonly property real panelWidth: panelActive ? geometry.width : 0
    readonly property real attachY: panelActive ? geometry.attachY : barY + barHeight
    readonly property real shoulderBottomY: panelActive ? geometry.shoulderBottomY : attachY
    readonly property real shoulderHeight: panelActive ? geometry.shoulderHeight : 0
    readonly property real leftShoulderWidth: panelActive ? geometry.leftShoulderWidth : 0
    readonly property real rightShoulderWidth: panelActive ? geometry.rightShoulderWidth : 0
    readonly property real bodyY: panelActive ? geometry.bodyY : shoulderBottomY
    readonly property real bodyHeight: panelActive ? geometry.contentHeight : 0
    readonly property real bodyWidth: panelActive ? geometry.width : 0
    readonly property real bodyRadius: panelActive
                                     ? clampRadius(geometry.lowerRadius, bodyWidth, bodyHeight)
                                     : 0
    readonly property real neckX: panelX
    readonly property real neckY: attachY
    readonly property real neckWidth: panelWidth
    readonly property real neckHeight: panelActive ? positive(bodyY - attachY) : 0
    readonly property real innerBorder: Math.max(0, borderThickness)

    readonly property QtObject outer: QtObject {
        readonly property bool panelActive: root.panelActive
        readonly property real barX: root.barX
        readonly property real barY: root.barY
        readonly property real barWidth: root.barWidth
        readonly property real barHeight: root.barHeight
        readonly property real barRadius: root.barRadius

        readonly property real neckX: root.panelActive ? root.neckX : 0
        readonly property real neckY: root.panelActive ? root.neckY : 0
        readonly property real neckWidth: root.panelActive ? root.neckWidth : 0
        readonly property real neckHeight: root.panelActive ? root.neckHeight : 0

        readonly property real bodyX: root.panelActive ? root.panelX : 0
        readonly property real bodyY: root.panelActive ? root.bodyY : 0
        readonly property real bodyWidth: root.panelActive ? root.bodyWidth : 0
        readonly property real bodyHeight: root.panelActive ? root.bodyHeight : 0
        readonly property real bodyRadius: root.panelActive ? root.bodyRadius : 0

        readonly property real leftShoulderClipX: root.panelActive
                                                  ? root.panelX - root.leftShoulderWidth
                                                  : 0
        readonly property real leftShoulderClipY: root.panelActive ? root.attachY : 0
        readonly property real leftShoulderClipWidth: root.panelActive ? root.leftShoulderWidth : 0
        readonly property real leftShoulderClipHeight: root.panelActive ? root.shoulderHeight : 0

        readonly property real rightShoulderClipX: root.panelActive
                                                   ? root.panelX + root.panelWidth
                                                   : 0
        readonly property real rightShoulderClipY: root.panelActive ? root.attachY : 0
        readonly property real rightShoulderClipWidth: root.panelActive ? root.rightShoulderWidth : 0
        readonly property real rightShoulderClipHeight: root.panelActive ? root.shoulderHeight : 0
    }

    readonly property QtObject inner: QtObject {
        readonly property bool panelActive: root.panelActive
        readonly property real barX: root.barX + root.innerBorder
        readonly property real barY: root.barY + root.innerBorder
        readonly property real barWidth: root.positive(root.barWidth - root.innerBorder * 2)
        readonly property real barHeight: root.positive(root.barHeight - root.innerBorder * 2)
        readonly property real barRadius: root.clampRadius(
            root.barRadius - root.innerBorder,
            barWidth,
            barHeight
        )

        readonly property real neckX: root.panelActive ? root.panelX + root.innerBorder : 0
        readonly property real neckY: root.panelActive ? root.attachY - root.innerBorder : 0
        readonly property real neckWidth: root.panelActive
                                          ? root.positive(root.panelWidth - root.innerBorder * 2)
                                          : 0
        readonly property real neckHeight: root.panelActive
                                           ? root.positive(root.neckHeight + root.innerBorder)
                                           : 0

        readonly property real bodyX: root.panelActive ? root.panelX + root.innerBorder : 0
        readonly property real bodyY: root.panelActive ? root.bodyY : 0
        readonly property real bodyWidth: root.panelActive
                                          ? root.positive(root.bodyWidth - root.innerBorder * 2)
                                          : 0
        readonly property real bodyHeight: root.panelActive
                                           ? root.positive(root.bodyHeight - root.innerBorder)
                                           : 0
        readonly property real bodyRadius: root.panelActive
                                           ? root.clampRadius(
                                                 root.bodyRadius - root.innerBorder,
                                                 bodyWidth,
                                                 bodyHeight
                                             )
                                           : 0

        readonly property real leftShoulderClipX: root.panelActive
                                                  ? root.panelX
                                                    - root.positive(
                                                          root.leftShoulderWidth - root.innerBorder
                                                      )
                                                  : 0
        readonly property real leftShoulderClipY: root.panelActive ? root.attachY + root.innerBorder : 0
        readonly property real leftShoulderClipWidth: root.panelActive
                                                      ? root.positive(
                                                            root.leftShoulderWidth - root.innerBorder
                                                        )
                                                      : 0
        readonly property real leftShoulderClipHeight: root.panelActive
                                                       ? root.positive(
                                                             root.shoulderHeight - root.innerBorder
                                                         )
                                                       : 0

        readonly property real rightShoulderClipX: root.panelActive ? root.panelX + root.panelWidth : 0
        readonly property real rightShoulderClipY: root.panelActive ? root.attachY + root.innerBorder : 0
        readonly property real rightShoulderClipWidth: root.panelActive
                                                       ? root.positive(
                                                             root.rightShoulderWidth - root.innerBorder
                                                         )
                                                       : 0
        readonly property real rightShoulderClipHeight: root.panelActive
                                                        ? root.positive(
                                                              root.shoulderHeight - root.innerBorder
                                                          )
                                                        : 0
    }
}
