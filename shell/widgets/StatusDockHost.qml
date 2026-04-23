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
    readonly property real capsuleWidth: statusRow.implicitWidth + Theme.statusCapsulePadX * 2
    readonly property real panelWidth: Math.max(
        capsuleWidth,
        Math.min(Theme.rightPopupWidth, availableWidth)
    )
    readonly property real _preferredX: {
        if (!triggerItem || !root.parent)
            return 0;
        var p = triggerItem.mapToItem(root.parent, 0, 0);
        return p.x + triggerItem.width - root.panelWidth;
    }
    readonly property real _minX: barItem ? barItem.x + Theme.panelEdgeInset : 0
    readonly property real _maxX: barItem
                                  ? barItem.x + barItem.width - Theme.panelEdgeInset - root.panelWidth
                                  : _preferredX
    readonly property real _triggerLocalX: {
        if (!triggerItem || !root.parent)
            return root.panelWidth - root.capsuleWidth;
        var p = triggerItem.mapToItem(root.parent, 0, 0);
        return p.x - root.x;
    }
    readonly property real capsuleX: Math.max(
        0,
        Math.min(root.panelWidth - root.capsuleWidth, root._triggerLocalX)
    )
    readonly property real leftShoulderWidth: Math.max(0, root.capsuleX)
    readonly property real rightShoulderWidth: Math.max(
        0,
        root.panelWidth - (root.capsuleX + root.capsuleWidth)
    )
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
    readonly property Item blurNeckItem: blurNeck
    readonly property Item blurLeftNotchItem: blurLeftNotch
    readonly property Item blurRightNotchItem: blurRightNotch

    x: Math.max(root._minX, Math.min(root._maxX, root._preferredX))
    y: barItem ? barItem.y : 0
    width: panelWidth
    height: Theme.barHeight + panelOverflowHeight
    z: 2

    function togglePanel(name) {
        root.toggleRequested(name);
        root.activePanel = root.activePanel === name ? "" : name;
    }

    function close() {
        root.activePanel = "";
    }

    onPanelBodyHeightChanged: shellCanvas.requestPaint()
    onCapsuleXChanged: shellCanvas.requestPaint()
    onCapsuleWidthChanged: shellCanvas.requestPaint()
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
        x: 0
        y: Theme.barHeight
        width: root.width
        height: root.panelOverflowHeight
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
            id: blurNeck
            x: root.capsuleX
            y: 0
            width: root.capsuleWidth
            height: Math.min(Theme.statusDockShoulderDepth, panelShell.height)
            visible: false
        }

        Item {
            id: blurLeftNotch
            readonly property real _width: root.leftShoulderWidth > 0
                                           ? Math.max(
                                                 Theme.statusDockShoulderDepth * 2,
                                                 root.leftShoulderWidth * 2
                                             )
                                           : 0

            x: root.capsuleX - (_width / 2)
            y: -Theme.statusDockShoulderDepth
            width: _width
            height: Theme.statusDockShoulderDepth * 2
            visible: false
        }

        Item {
            id: blurRightNotch
            readonly property real _width: root.rightShoulderWidth > 0
                                           ? Math.max(
                                                 Theme.statusDockShoulderDepth * 2,
                                                 root.rightShoulderWidth * 2
                                             )
                                           : 0

            x: root.capsuleX + root.capsuleWidth - (_width / 2)
            y: -Theme.statusDockShoulderDepth
            width: _width
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
                var d = Math.min(Theme.statusDockShoulderDepth, h);
                var r = Math.min(Theme.statusDockLowerRadius, Math.max(0, h / 2));
                var nx = root.capsuleX;
                var nw = root.capsuleWidth;
                var right = w - (nx + nw);

                var leftCp1X = Math.max(0, nx * 0.48);
                var rightCp1X = nw + nx + Math.max(6, right * 0.52);

                ctx.beginPath();
                ctx.moveTo(nx, 0);
                ctx.lineTo(nx + nw, 0);
                ctx.bezierCurveTo(
                    rightCp1X,
                    0,
                    w,
                    d * 0.42,
                    w,
                    d
                );
                ctx.lineTo(w, h - r);
                ctx.quadraticCurveTo(w, h, w - r, h);
                ctx.lineTo(r, h);
                ctx.quadraticCurveTo(0, h, 0, h - r);
                ctx.lineTo(0, d);
                ctx.bezierCurveTo(
                    0,
                    d * 0.42,
                    leftCp1X,
                    0,
                    nx,
                    0
                );
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

    Rectangle {
        id: capsule
        x: root.capsuleX
        y: 0
        width: root.capsuleWidth
        height: Theme.statusCapsuleHeight
        radius: Theme.statusCapsuleRadius
        color: Theme.statusDockFill
        border.color: Theme.statusDockBorder
        border.width: 1

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onPressed: function(mouse) { mouse.accepted = true; }
            onClicked: function(mouse) { mouse.accepted = true; }
        }

        Row {
            id: statusRow
            anchors.centerIn: parent
            spacing: Theme.spaceSm

            BatteryIndicator {
                anchors.verticalCenter: parent.verticalCenter
                onClicked: root.togglePanel("battery")
            }

            NetworkIndicator {
                anchors.verticalCenter: parent.verticalCenter
                onClicked: root.togglePanel("network")
            }

            BluetoothIndicator {
                anchors.verticalCenter: parent.verticalCenter
                onClicked: root.togglePanel("bluetooth")
            }

            VolumeIndicator {
                anchors.verticalCenter: parent.verticalCenter
                onClicked: root.togglePanel("volume")
            }

            NotificationButton {
                anchors.verticalCenter: parent.verticalCenter
                onToggled: root.togglePanel("notification")
            }
        }
    }
}
