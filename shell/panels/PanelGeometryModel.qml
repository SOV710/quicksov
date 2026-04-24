// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

QtObject {
    id: root

    property string surfaceName: "panel"
    property Item barItem: null
    property Item triggerItem: null
    property Item coordinateItem: null
    property string alignmentMode: "right"
    property real preferredXOffset: 0
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
    property bool _revealReady: false
    property real _settledBodyTarget: 0

    function _reset_reveal_state() {
        _revealReady = false;
        _settledBodyTarget = 0;
        _revealSettleTimer.stop();
    }

    function _schedule_reveal() {
        if (!open || measuredBodyTarget <= 0)
            return;

        _revealReady = false;
        _revealSettleTimer.restart();
        DebugVisuals.logTransition(root.surfaceName, "popup-open", {
            event: "reveal-settle-scheduled",
            measuredBodyTarget: root.measuredBodyTarget
        });
    }

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
    readonly property real measuredBodyTarget: Math.min(contentImplicitHeight, maxBodyHeight)
    readonly property real targetBodyHeight: open ? (_revealReady ? measuredBodyTarget : 0) : 0
    property real bodyHeight: targetBodyHeight
    readonly property bool active: open || bodyHeight > 0.5 || bodyHeightAnimation.running
    readonly property real visualRevealThreshold: Math.min(_settledBodyTarget, lowerRadius * 2)
    readonly property bool bodyVisualVisible: active
                                             && _settledBodyTarget > 0.5
                                             && bodyHeight >= Math.max(1, visualRevealThreshold - 0.5)
    readonly property bool contentVisible: active
                                           && (open
                                                   ? (_settledBodyTarget > 0.5
                                                          && bodyHeight >= _settledBodyTarget - 0.5)
                                                   : bodyHeight > Math.max(1, visualRevealThreshold - 0.5))
    readonly property bool hasBarMapping: barItem !== null && coordinateItem !== null
    readonly property bool hasTriggerMapping: triggerItem !== null && coordinateItem !== null

    readonly property real barLeft: _barSceneX
    readonly property real barRight: hasBarMapping ? _barSceneX + _barSceneWidth : x + width
    readonly property real preferredX: {
        if (hasTriggerMapping) {
            if (alignmentMode === "center")
                return _triggerSceneX + (_triggerSceneWidth - panelWidth) / 2 + preferredXOffset;
            return _triggerSceneX + _triggerSceneWidth - panelWidth + preferredXOffset;
        }
        return barRight - panelWidth + preferredXOffset;
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
            duration: DebugVisuals.freezePanelBodyHeightToFinal
                      ? 0
                      : DebugVisuals.duration(Theme.statusDockRevealDuration)
            easing.type: Easing.OutCubic

            onRunningChanged: {
                DebugVisuals.logTransition(root.surfaceName, root.open ? "popup-open" : "popup-close", {
                    event: running ? "body-height-animation-start" : "body-height-animation-stop",
                    active: root.active,
                    bodyHeight: root.bodyHeight,
                    contentImplicitHeight: root.contentImplicitHeight,
                    maxBodyHeight: root.maxBodyHeight,
                    measuredBodyTarget: root.measuredBodyTarget,
                    revealReady: root._revealReady,
                    settledBodyTarget: root._settledBodyTarget,
                    targetBodyHeight: root.targetBodyHeight
                });
            }
        }
    }

    onBarItemChanged: refresh_mappings()
    onTriggerItemChanged: refresh_mappings()
    onCoordinateItemChanged: refresh_mappings()
    onOpenChanged: {
        if (root.open)
            root._schedule_reveal();
        else
            root._revealSettleTimer.stop();

        DebugVisuals.logTransition(root.surfaceName, root.open ? "popup-open" : "popup-close", {
            active: root.active,
            bodyHeight: root.bodyHeight,
            contentImplicitHeight: root.contentImplicitHeight,
            event: root.open ? "open-changed-true" : "open-changed-false",
            maxBodyHeight: root.maxBodyHeight,
            measuredBodyTarget: root.measuredBodyTarget,
            revealReady: root._revealReady,
            settledBodyTarget: root._settledBodyTarget,
            targetBodyHeight: root.targetBodyHeight
        });
    }
    onContentImplicitHeightChanged: {
        DebugVisuals.logTransition(root.surfaceName, root.open ? "popup-open" : "popup-close", {
            active: root.active,
            bodyHeight: root.bodyHeight,
            contentImplicitHeight: root.contentImplicitHeight,
            event: "content-implicit-height-changed",
            maxBodyHeight: root.maxBodyHeight,
            measuredBodyTarget: root.measuredBodyTarget,
            revealReady: root._revealReady,
            settledBodyTarget: root._settledBodyTarget,
            targetBodyHeight: root.targetBodyHeight
        });
    }
    onMeasuredBodyTargetChanged: {
        if (root.open) {
            if (root._revealReady)
                root._settledBodyTarget = root.measuredBodyTarget;
            else
                root._schedule_reveal();
        }

        DebugVisuals.logTransition(root.surfaceName, root.open ? "popup-open" : "popup-close", {
            active: root.active,
            bodyHeight: root.bodyHeight,
            contentImplicitHeight: root.contentImplicitHeight,
            event: "measured-body-target-changed",
            maxBodyHeight: root.maxBodyHeight,
            measuredBodyTarget: root.measuredBodyTarget,
            revealReady: root._revealReady,
            settledBodyTarget: root._settledBodyTarget,
            targetBodyHeight: root.targetBodyHeight
        });
    }
    onBodyHeightChanged: {
        DebugVisuals.logTransition(root.surfaceName, root.open ? "popup-open" : "popup-close", {
            active: root.active,
            bodyVisualVisible: root.bodyVisualVisible,
            bodyHeight: root.bodyHeight,
            contentVisible: root.contentVisible,
            contentImplicitHeight: root.contentImplicitHeight,
            event: "body-height-changed",
            maxBodyHeight: root.maxBodyHeight,
            measuredBodyTarget: root.measuredBodyTarget,
            revealReady: root._revealReady,
            settledBodyTarget: root._settledBodyTarget,
            targetBodyHeight: root.targetBodyHeight
        });
    }
    onActiveChanged: {
        if (!root.active && !root.open)
            root._reset_reveal_state();
    }
    Component.onCompleted: {
        refresh_mappings();
        if (root.open)
            root._schedule_reveal();
        DebugVisuals.logTransition(root.surfaceName, root.open ? "popup-open" : "popup-close", {
            active: root.active,
            bodyHeight: root.bodyHeight,
            contentImplicitHeight: root.contentImplicitHeight,
            event: "component-completed",
            maxBodyHeight: root.maxBodyHeight,
            measuredBodyTarget: root.measuredBodyTarget,
            revealReady: root._revealReady,
            settledBodyTarget: root._settledBodyTarget,
            targetBodyHeight: root.targetBodyHeight
        });
    }

    property var _revealSettleTimer: Timer {
        interval: 24
        repeat: false
        running: false

        onTriggered: {
            if (!root.open || root.measuredBodyTarget <= 0)
                return;

            root._settledBodyTarget = root.measuredBodyTarget;
            root._revealReady = true;
            DebugVisuals.logTransition(root.surfaceName, "popup-open", {
                event: "reveal-settle-ready",
                measuredBodyTarget: root.measuredBodyTarget,
                settledBodyTarget: root._settledBodyTarget
            });
        }
    }

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
