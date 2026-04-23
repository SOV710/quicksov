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
    readonly property Item blurLeftCutoutItem: blurLeftCutout
    readonly property Item blurRightCutoutItem: blurRightCutout

    x: Math.max(root._minX, Math.min(root._maxX, root._preferredX))
    y: barItem ? barItem.y + barItem.height : 0
    width: panelWidth
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
            x: 0
            y: 0
            width: panelShell.width
            height: panelShell.height
            visible: false
        }

        Item {
            id: blurLeftCutout
            x: -Theme.statusDockShoulderDepth
            y: -Theme.statusDockShoulderDepth
            width: Theme.statusDockShoulderDepth * 2
            height: Theme.statusDockShoulderDepth * 2
            visible: false
        }

        Item {
            id: blurRightCutout
            x: panelShell.width - Theme.statusDockShoulderDepth
            y: -Theme.statusDockShoulderDepth
            width: Theme.statusDockShoulderDepth * 2
            height: Theme.statusDockShoulderDepth * 2
            visible: false
        }

        Canvas {
            id: shellCanvas
            anchors.fill: parent
            visible: panelShell.visible
            antialiasing: true

            function drawPath(ctx) {
                var w = width;
                var h = height;
                var d = Math.min(Theme.statusDockShoulderDepth, Math.max(0, h));
                var r = Math.min(Theme.statusDockLowerRadius, Math.max(0, h / 2));

                ctx.beginPath();
                ctx.moveTo(d, 0);
                ctx.lineTo(w - d, 0);
                ctx.quadraticCurveTo(w, 0, w, d);
                ctx.lineTo(w, h - r);
                ctx.quadraticCurveTo(w, h, w - r, h);
                ctx.lineTo(r, h);
                ctx.quadraticCurveTo(0, h, 0, h - r);
                ctx.lineTo(0, d);
                ctx.quadraticCurveTo(0, 0, d, 0);
                ctx.closePath();
            }

            onPaint: {
                var ctx = getContext("2d");
                ctx.clearRect(0, 0, width, height);
                drawPath(ctx);
                ctx.fillStyle = root.shellFill;
                ctx.fill();
                ctx.lineWidth = 1;
                ctx.strokeStyle = root.shellStroke;
                ctx.stroke();
            }
        }

        Item {
            id: contentViewport
            x: 0
            y: Theme.statusDockShoulderDepth
            width: parent.width
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
