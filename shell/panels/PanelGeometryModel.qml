// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

QtObject {
    id: root

    property Item barItem: null
    property Item triggerItem: null
    property Item coordinateItem: null
    property string alignmentMode: "right"
    property real preferredWidth: 0
    property real availableWidth: 0
    property real maxBodyHeight: 0
    property real contentImplicitHeight: 0
    property bool open: false
    property int shoulderDepth: Theme.statusDockShoulderDepth
    property int lowerRadius: Theme.statusDockLowerRadius
    property int seamOverlap: Theme.statusDockSeamOverlap

    readonly property real panelWidth: Math.max(0, Math.min(preferredWidth, availableWidth))
    property real bodyHeight: open ? Math.min(contentImplicitHeight, maxBodyHeight) : 0
    readonly property bool active: open || bodyHeight > 0.5 || bodyHeightAnimation.running

    readonly property real barLeft: {
        if (barItem && coordinateItem) {
            var p = barItem.mapToItem(coordinateItem, 0, 0);
            return p.x;
        }
        return 0;
    }
    readonly property real barRight: {
        if (barItem && coordinateItem) {
            var p = barItem.mapToItem(coordinateItem, 0, 0);
            return p.x + barItem.width;
        }
        return x + width;
    }
    readonly property real preferredX: {
        if (triggerItem && coordinateItem) {
            var p = triggerItem.mapToItem(coordinateItem, 0, 0);
            if (alignmentMode === "center")
                return p.x + (triggerItem.width - panelWidth) / 2;
            return p.x + triggerItem.width - panelWidth;
        }
        return barRight - panelWidth;
    }
    readonly property real x: Math.max(barLeft, Math.min(barRight - panelWidth, preferredX))
    readonly property real y: Math.max(0, attachY - seamOverlap)
    readonly property real width: panelWidth
    readonly property real height: shoulderDepth + bodyHeight
    readonly property real contentX: x
    readonly property real contentY: shoulderBottomY
    readonly property real contentWidth: width
    readonly property real contentHeight: bodyHeight
    readonly property real attachY: barItem && coordinateItem
                                     ? barItem.mapToItem(coordinateItem, 0, 0).y + barItem.height
                                     : 0
    readonly property real shoulderBottomY: y + shoulderDepth
    readonly property real shoulderHeight: Math.max(0, shoulderBottomY - attachY)
    readonly property real bodyY: contentY
    readonly property real bodyBottomY: bodyY + bodyHeight
    readonly property real leftShoulderWidth: Math.max(
        0,
        Math.min(shoulderDepth, x - barLeft)
    )
    readonly property real rightShoulderWidth: Math.max(
        0,
        Math.min(shoulderDepth, barRight - (x + width))
    )
    readonly property real leftShoulderTopX: x - leftShoulderWidth
    readonly property real rightShoulderTopX: x + width + rightShoulderWidth
    readonly property real leftAttachX: leftShoulderTopX
    readonly property real rightAttachX: rightShoulderTopX
    readonly property real topLeftRadius: leftShoulderWidth
    readonly property real topRightRadius: rightShoulderWidth

    Behavior on bodyHeight {
        NumberAnimation {
            id: bodyHeightAnimation
            duration: Theme.statusDockRevealDuration
            easing.type: Easing.OutCubic
        }
    }
}
