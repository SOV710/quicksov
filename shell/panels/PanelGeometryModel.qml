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
    property real _barSceneX: 0
    property real _barSceneY: 0
    property real _barSceneWidth: 0
    property real _barSceneHeight: 0
    property real _triggerSceneX: 0
    property real _triggerSceneY: 0
    property real _triggerSceneWidth: 0
    property real _triggerSceneHeight: 0

    function map_item(item) {
        if (!item || !coordinateItem)
            return Qt.point(0, 0);

        return item.mapToItem(coordinateItem, 0, 0);
    }

    function refresh_bar_mapping() {
        if (!barItem || !coordinateItem) {
            _barSceneX = 0;
            _barSceneY = 0;
            _barSceneWidth = 0;
            _barSceneHeight = 0;
            return;
        }

        var point = map_item(barItem);
        _barSceneX = point.x;
        _barSceneY = point.y;
        _barSceneWidth = barItem.width;
        _barSceneHeight = barItem.height;
    }

    function refresh_trigger_mapping() {
        if (!triggerItem || !coordinateItem) {
            _triggerSceneX = 0;
            _triggerSceneY = 0;
            _triggerSceneWidth = 0;
            _triggerSceneHeight = 0;
            return;
        }

        var point = map_item(triggerItem);
        _triggerSceneX = point.x;
        _triggerSceneY = point.y;
        _triggerSceneWidth = triggerItem.width;
        _triggerSceneHeight = triggerItem.height;
    }

    function refresh_mappings() {
        refresh_bar_mapping();
        refresh_trigger_mapping();
    }

    readonly property real panelWidth: Math.max(0, Math.min(preferredWidth, availableWidth))
    property real bodyHeight: open ? Math.min(contentImplicitHeight, maxBodyHeight) : 0
    readonly property bool active: open || bodyHeight > 0.5 || bodyHeightAnimation.running
    readonly property bool hasBarMapping: barItem !== null && coordinateItem !== null
    readonly property bool hasTriggerMapping: triggerItem !== null && coordinateItem !== null

    readonly property real barLeft: _barSceneX
    readonly property real barRight: hasBarMapping ? _barSceneX + _barSceneWidth : x + width
    readonly property real preferredX: {
        if (hasTriggerMapping) {
            if (alignmentMode === "center")
                return _triggerSceneX + (_triggerSceneWidth - panelWidth) / 2;
            return _triggerSceneX + _triggerSceneWidth - panelWidth;
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
    readonly property real attachY: hasBarMapping ? _barSceneY + _barSceneHeight : 0
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

    onBarItemChanged: refresh_mappings()
    onTriggerItemChanged: refresh_mappings()
    onCoordinateItemChanged: refresh_mappings()
    Component.onCompleted: refresh_mappings()

    property var _barItemConnections: Connections {
        target: root.barItem

        function onXChanged() {
            root.refresh_mappings();
        }

        function onYChanged() {
            root.refresh_mappings();
        }

        function onWidthChanged() {
            root.refresh_mappings();
        }

        function onHeightChanged() {
            root.refresh_mappings();
        }
    }

    property var _triggerItemConnections: Connections {
        target: root.triggerItem

        function onXChanged() {
            root.refresh_trigger_mapping();
        }

        function onYChanged() {
            root.refresh_trigger_mapping();
        }

        function onWidthChanged() {
            root.refresh_trigger_mapping();
        }

        function onHeightChanged() {
            root.refresh_trigger_mapping();
        }
    }

    property var _coordinateItemConnections: Connections {
        target: root.coordinateItem

        function onXChanged() {
            root.refresh_mappings();
        }

        function onYChanged() {
            root.refresh_mappings();
        }

        function onWidthChanged() {
            root.refresh_mappings();
        }

        function onHeightChanged() {
            root.refresh_mappings();
        }
    }
}
