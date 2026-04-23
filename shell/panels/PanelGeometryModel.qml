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
    readonly property real y: {
        if (barItem && coordinateItem) {
            var p = barItem.mapToItem(coordinateItem, 0, 0);
            // Start the panel shell below the bar body so the shoulder junction
            // visually grows out from the underside instead of the center.
            return p.y + barItem.height - seamOverlap + (barItem.height / 2);
        }
        return 0;
    }
    readonly property real width: panelWidth
    readonly property real height: shoulderDepth + bodyHeight
    readonly property real contentX: x
    readonly property real contentY: y + shoulderDepth
    readonly property real contentWidth: width
    readonly property real contentHeight: bodyHeight

    readonly property real topLeftRadius: Math.max(
        0,
        Math.min(shoulderDepth, x - barLeft)
    )
    readonly property real topRightRadius: Math.max(
        0,
        Math.min(shoulderDepth, barRight - (x + width))
    )

    Behavior on bodyHeight {
        NumberAnimation {
            id: bodyHeightAnimation
            duration: Theme.statusDockRevealDuration
            easing.type: Easing.OutCubic
        }
    }
}
