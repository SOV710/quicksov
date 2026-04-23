// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../overlays"

Item {
    id: root

    signal toggleRequested(string name)

    property Item barItem: null
    property Item triggerItem: null
    property real availableWidth: Theme.rightPopupWidth
    property real maxPanelHeight: Theme.rightPopupMaxHeight
    property string activePanel: ""

    readonly property bool panelVisible: activePanel !== ""
    readonly property real panelWidth: Math.min(Theme.rightPopupWidth, availableWidth)
    readonly property real _preferredX: {
        if (triggerItem && root.parent) {
            var p = triggerItem.mapToItem(root.parent, 0, 0);
            return p.x + triggerItem.width - root.panelWidth;
        }
        if (barItem)
            return barItem.x + barItem.width - Theme.statusPopupRightInset - root.panelWidth;
        return 0;
    }
    readonly property real _minX: barItem ? barItem.x + Theme.panelEdgeInset : 0
    readonly property real _maxX: barItem
                                  ? barItem.x + barItem.width - Theme.panelEdgeInset - root.panelWidth
                                  : _preferredX
    readonly property color shellFill: Theme.statusDockFill
    readonly property color shellStroke: Theme.statusDockBorder
    readonly property real _barLeft: {
        if (barItem && root.parent) {
            var p = barItem.mapToItem(root.parent, 0, 0);
            return p.x;
        }
        return 0;
    }
    readonly property real _barRight: {
        if (barItem && root.parent) {
            var p = barItem.mapToItem(root.parent, 0, 0);
            return p.x + barItem.width;
        }
        return root._panelX + root.panelWidth;
    }
    readonly property real _panelX: Math.max(root._minX, Math.min(root._maxX, root._preferredX))
    readonly property real _leftShoulderRadius: Math.max(
        0,
        Math.min(Theme.statusDockShoulderDepth, root._panelX - root._barLeft)
    )
    readonly property real _rightShoulderRadius: Math.max(
        0,
        Math.min(Theme.statusDockShoulderDepth, root._barRight - (root._panelX + root.panelWidth))
    )

    readonly property real currentPanelImplicitHeight: {
        switch (root.activePanel) {
        case "battery":
            return batteryPanel.implicitHeight;
        case "network":
            return networkPanel.implicitHeight;
        case "bluetooth":
            return bluetoothPanel.implicitHeight;
        case "volume":
            return volumePanel.implicitHeight;
        case "notification":
            return notificationPanel.implicitHeight;
        default:
            return 0;
        }
    }
    property real panelBodyHeight: panelVisible
                                   ? Math.min(root.currentPanelImplicitHeight, root.maxPanelHeight)
                                   : 0
    readonly property real panelOverflowHeight: panelBodyHeight > 0
                                                ? Theme.statusDockShoulderDepth + panelBodyHeight
                                                : 0
    readonly property bool shellVisible: panelOverflowHeight > 0.5

    readonly property Item blurBodyItem: blurBody
    readonly property Item blurLeftCornerSquareItem: blurLeftCornerSquare
    readonly property Item blurRightCornerSquareItem: blurRightCornerSquare
    readonly property Item blurLeftShoulderArcItem: blurLeftShoulderArc
    readonly property Item blurRightShoulderArcItem: blurRightShoulderArc

    x: root._panelX - root._leftShoulderRadius
    y: barItem ? barItem.y + barItem.height - Theme.statusDockSeamOverlap : 0
    width: panelWidth + root._leftShoulderRadius + root._rightShoulderRadius
    height: panelOverflowHeight
    z: 2

    function togglePanel(name) {
        root.toggleRequested(name);
        root.activePanel = root.activePanel === name ? "" : name;
    }

    function close() {
        root.activePanel = "";
    }

    onPanelBodyHeightChanged: shellCanvas.requestPaint()
    onShellFillChanged: shellCanvas.requestPaint()
    onShellStrokeChanged: shellCanvas.requestPaint()
    onWidthChanged: shellCanvas.requestPaint()
    onXChanged: shellCanvas.requestPaint()

    Behavior on panelBodyHeight {
        NumberAnimation {
            duration: Theme.statusDockRevealDuration
            easing.type: Easing.OutCubic
        }
    }

    Item {
        id: panelShell
        anchors.fill: parent
        visible: root.shellVisible

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onPressed: function(mouse) { mouse.accepted = true; }
            onClicked: function(mouse) { mouse.accepted = true; }
        }

        Item {
            id: blurBody
            x: root._leftShoulderRadius
            y: 0
            width: root.panelWidth
            height: panelShell.height
            visible: false
        }

        Item {
            id: blurLeftCornerSquare
            x: 0
            y: 0
            width: root._leftShoulderRadius
            height: root._leftShoulderRadius
            visible: false
        }

        Item {
            id: blurRightCornerSquare
            x: panelShell.width - root._rightShoulderRadius
            y: 0
            width: root._rightShoulderRadius
            height: root._rightShoulderRadius
            visible: false
        }

        Item {
            id: blurLeftShoulderArc
            x: 0
            y: 0
            width: root._leftShoulderRadius * 2
            height: root._leftShoulderRadius * 2
            visible: false
        }

        Item {
            id: blurRightShoulderArc
            x: panelShell.width - (root._rightShoulderRadius * 2)
            y: 0
            width: root._rightShoulderRadius * 2
            height: root._rightShoulderRadius * 2
            visible: false
        }

        Canvas {
            id: shellCanvas
            anchors.fill: parent
            visible: panelShell.visible
            antialiasing: true

            function drawFillPath(ctx) {
                var w = width;
                var h = height;
                var left = root._leftShoulderRadius;
                var right = root._rightShoulderRadius;
                var panelLeft = left;
                var panelRight = left + root.panelWidth;
                var leftR = Math.min(left, Theme.statusDockShoulderDepth, Math.max(0, h));
                var rightR = Math.min(right, Theme.statusDockShoulderDepth, Math.max(0, h));
                var r = Math.min(Theme.statusDockLowerRadius, Math.max(0, h / 2), Math.max(0, root.panelWidth / 2));

                ctx.beginPath();
                ctx.moveTo(0, 0);
                if (leftR > 0)
                    ctx.quadraticCurveTo(panelLeft, 0, panelLeft, leftR);
                else
                    ctx.lineTo(panelLeft, 0);
                ctx.lineTo(panelLeft, h - r);
                ctx.quadraticCurveTo(panelLeft, h, panelLeft + r, h);
                ctx.lineTo(panelRight - r, h);
                ctx.quadraticCurveTo(panelRight, h, panelRight, h - r);
                ctx.lineTo(panelRight, rightR);
                if (rightR > 0)
                    ctx.quadraticCurveTo(panelRight, 0, w, 0);
                else
                    ctx.lineTo(w, 0);
                ctx.lineTo(0, 0);
                ctx.closePath();
            }

            function drawStrokePath(ctx) {
                var w = width;
                var h = height;
                var left = root._leftShoulderRadius;
                var right = root._rightShoulderRadius;
                var panelLeft = left;
                var panelRight = left + root.panelWidth;
                var leftR = Math.min(left, Theme.statusDockShoulderDepth, Math.max(0, h));
                var rightR = Math.min(right, Theme.statusDockShoulderDepth, Math.max(0, h));
                var r = Math.min(Theme.statusDockLowerRadius, Math.max(0, h / 2), Math.max(0, root.panelWidth / 2));

                ctx.beginPath();
                ctx.moveTo(0, 0);
                if (leftR > 0)
                    ctx.quadraticCurveTo(panelLeft, 0, panelLeft, leftR);
                else
                    ctx.lineTo(panelLeft, 0);
                ctx.lineTo(panelLeft, h - r);
                ctx.quadraticCurveTo(panelLeft, h, panelLeft + r, h);
                ctx.lineTo(panelRight - r, h);
                ctx.quadraticCurveTo(panelRight, h, panelRight, h - r);
                ctx.lineTo(panelRight, rightR);
                if (rightR > 0)
                    ctx.quadraticCurveTo(panelRight, 0, w, 0);
                else
                    ctx.lineTo(w, 0);
            }

            onPaint: {
                var ctx = getContext("2d");
                ctx.clearRect(0, 0, width, height);
                drawFillPath(ctx);
                ctx.fillStyle = root.shellFill;
                ctx.fill();
                drawStrokePath(ctx);
                ctx.lineWidth = 1;
                ctx.strokeStyle = root.shellStroke;
                ctx.stroke();
            }
        }

        Item {
            id: contentViewport
            x: root._leftShoulderRadius
            y: Theme.statusDockShoulderDepth
            width: root.panelWidth
            height: root.panelBodyHeight
            clip: true
            visible: height > 0

            BatteryPopup {
                id: batteryPanel
                width: parent.width
                visible: root.activePanel === "battery"
            }

            NetworkPopup {
                id: networkPanel
                width: parent.width
                visible: root.activePanel === "network"
            }

            BluetoothPopup {
                id: bluetoothPanel
                width: parent.width
                visible: root.activePanel === "bluetooth"
            }

            VolumePopup {
                id: volumePanel
                width: parent.width
                visible: root.activePanel === "volume"
            }

            NotificationCenter {
                id: notificationPanel
                width: parent.width
                visible: root.activePanel === "notification"
            }
        }
    }
}
