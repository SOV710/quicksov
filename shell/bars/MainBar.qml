// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import ".."
import "../services"
import "../widgets"
import "../overlays"

Scope {
    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: bar

            required property var modelData
            screen: modelData
            visible: true

            anchors.top: true
            anchors.left: true
            anchors.right: true

            margins {
                top: Theme.barOuterMargin
                left: Theme.barOuterMargin
                right: Theme.barOuterMargin
            }

            readonly property bool _anyPopupOpen: clockPopup.popupVisible
                                                 || notifCenter.visible
                                                 || bluetoothPopup.visible
                                                 || networkPopup.visible
                                                 || volumePopup.visible
                                                 || batteryPopup.visible

            property int _popupHeight: {
                var h = 0;
                if (clockPopup.popupVisible) h = Math.max(h, clockPopup.implicitHeight + Theme.popupGap);
                if (notifCenter.visible)     h = Math.max(h, notifCenter.implicitHeight + Theme.popupGap);
                if (bluetoothPopup.visible)  h = Math.max(h, bluetoothPopup.implicitHeight + Theme.popupGap);
                if (networkPopup.visible)    h = Math.max(h, networkPopup.implicitHeight + Theme.popupGap);
                if (volumePopup.visible)     h = Math.max(h, volumePopup.implicitHeight + Theme.popupGap);
                if (batteryPopup.visible)    h = Math.max(h, batteryPopup.implicitHeight + Theme.popupGap);
                return h;
            }

            implicitHeight: bar._anyPopupOpen && bar.screen
                            ? Math.max(
                                  Theme.barHeight + Theme.barOuterMargin + _popupHeight,
                                  bar.screen.height - Theme.barOuterMargin
                              )
                            : Theme.barHeight + Theme.barOuterMargin + _popupHeight

            exclusiveZone: Theme.barHeight
            color: "transparent"

            function closeAllPopups() {
                clockPopup.popupVisible = false;
                notifCenter.visible = false;
                bluetoothPopup.visible = false;
                networkPopup.visible = false;
                volumePopup.visible = false;
                batteryPopup.visible = false;
            }

            MouseArea {
                anchors.fill: parent
                visible: bar._anyPopupOpen
                acceptedButtons: Qt.AllButtons
                onClicked: bar.closeAllPopups()
            }

            Rectangle {
                id: barShadowWide
                z: 0
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    leftMargin: 4
                    rightMargin: 4
                    topMargin: 10
                }
                height: Theme.barHeight + 10
                radius: Theme.barRadius + 6
                color: Theme.withAlpha(Theme.barShadowColor, 0.32)
            }

            Rectangle {
                id: barShadow
                z: 0
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    topMargin: 6
                }
                height: Theme.barHeight + 6
                radius: Theme.barRadius + 4
                color: Theme.withAlpha(Theme.barShadowColor, 0.68)
            }

            Rectangle {
                id: barRect
                z: 1
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                }
                height: Theme.barHeight
                radius: Theme.barRadius
                color: Theme.withAlpha(Theme.bgSurface, Theme.opacityPanel)
                border.color: Theme.withAlpha(Theme.borderDefault, 0.82)
                border.width: 1

                Row {
                    id: leftZone
                    anchors {
                        left: parent.left
                        leftMargin: Theme.barPadX
                        verticalCenter: parent.verticalCenter
                    }
                    spacing: Theme.spaceLg

                    WorkspaceStrip {
                        id: workspaceStrip
                        outputName: bar.screen ? bar.screen.name : ""
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    WindowInfo {
                        id: windowInfo
                        anchors.verticalCenter: parent.verticalCenter
                        maxWidth: Math.max(
                            0,
                            Math.floor(
                                (barRect.width / 2) - (clockWidget.implicitWidth / 2) - Theme.spaceLg
                                - (Theme.barPadX + workspaceStrip.width + leftZone.spacing)
                            )
                        )
                    }
                }

                Clock {
                    id: clockWidget
                    anchors.centerIn: parent
                    onOpenPopup: {
                        var next = !clockPopup.popupVisible;
                        bar.closeAllPopups();
                        clockPopup.popupVisible = next;
                    }
                }

                Row {
                    id: rightZone
                    anchors {
                        right: parent.right
                        rightMargin: Theme.barPadX
                        verticalCenter: parent.verticalCenter
                    }
                    spacing: Theme.spaceSm

                    TrayHost {
                        id: trayHost
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Rectangle {
                        id: statusCapsule
                        height: Theme.statusCapsuleHeight
                        width: statusRow.implicitWidth + Theme.statusCapsulePadX * 2
                        radius: Theme.statusCapsuleRadius
                        color: Theme.statusCapsuleFill
                        border.color: Theme.statusCapsuleBorder
                        border.width: 1
                        anchors.verticalCenter: parent.verticalCenter

                        Row {
                            id: statusRow
                            anchors.centerIn: parent
                            spacing: Theme.spaceSm

                            BatteryIndicator {
                                id: batteryWidget
                                anchors.verticalCenter: parent.verticalCenter
                                onClicked: {
                                    var next = !batteryPopup.visible;
                                    bar.closeAllPopups();
                                    batteryPopup.visible = next;
                                }
                            }

                            NetworkIndicator {
                                id: networkWidget
                                anchors.verticalCenter: parent.verticalCenter
                                onClicked: {
                                    var next = !networkPopup.visible;
                                    bar.closeAllPopups();
                                    networkPopup.visible = next;
                                }
                            }

                            BluetoothIndicator {
                                id: bluetoothWidget
                                anchors.verticalCenter: parent.verticalCenter
                                onClicked: {
                                    var next = !bluetoothPopup.visible;
                                    bar.closeAllPopups();
                                    bluetoothPopup.visible = next;
                                }
                            }

                            VolumeIndicator {
                                id: volumeWidget
                                anchors.verticalCenter: parent.verticalCenter
                                onClicked: {
                                    var next = !volumePopup.visible;
                                    bar.closeAllPopups();
                                    volumePopup.visible = next;
                                }
                            }

                            NotificationButton {
                                id: notifWidget
                                anchors.verticalCenter: parent.verticalCenter
                                onToggled: {
                                    var next = !notifCenter.visible;
                                    bar.closeAllPopups();
                                    notifCenter.visible = next;
                                }
                            }
                        }
                    }
                }
            }

            ClockPopup {
                id: clockPopup
                z: 2
                availableWidth: Math.max(0, barRect.width - Theme.panelEdgeInset * 2)
                availableHeight: bar.screen
                                 ? Math.max(
                                       0,
                                       bar.screen.height - Theme.barHeight - Theme.barOuterMargin * 2 - Theme.panelEdgeInset * 2
                                   )
                                 : Theme.clockPanelMaxHeight
                anchors {
                    top: barRect.bottom
                    topMargin: Theme.popupGap
                    horizontalCenter: barRect.horizontalCenter
                }
            }

            Item {
                id: notifPopupAnchor
                x: barRect.x + rightZone.x + statusCapsule.x + statusRow.x + notifWidget.x
                y: barRect.y + rightZone.y + statusCapsule.y + statusRow.y + notifWidget.y
                width: notifWidget.width
                height: notifWidget.height
                visible: false
            }

            NotificationCenter {
                id: notifCenter
                z: 2
                visible: false
                width: Math.min(Theme.notificationPanelWidth, barRect.width - Theme.panelEdgeInset * 2)

                readonly property real _preferredX: notifPopupAnchor.x + (notifPopupAnchor.width - width) / 2
                readonly property real _minX: barRect.x + Theme.panelEdgeInset
                readonly property real _maxX: Math.max(_minX, barRect.x + barRect.width - Theme.panelEdgeInset - width)

                x: Math.max(_minX, Math.min(_preferredX, _maxX))
                y: barRect.y + barRect.height + Theme.popupGap
            }

            Item {
                id: bluetoothPopupAnchor
                x: barRect.x + rightZone.x + statusCapsule.x + statusRow.x + bluetoothWidget.x
                y: barRect.y + rightZone.y + statusCapsule.y + statusRow.y + bluetoothWidget.y
                width: bluetoothWidget.width
                height: bluetoothWidget.height
                visible: false
            }

            BluetoothPopup {
                id: bluetoothPopup
                z: 2
                visible: false
                width: Math.min(Theme.bluetoothPanelWidth, barRect.width - Theme.panelEdgeInset * 2)

                readonly property real _preferredX: bluetoothPopupAnchor.x + (bluetoothPopupAnchor.width - width) / 2
                readonly property real _minX: barRect.x + Theme.panelEdgeInset
                readonly property real _maxX: Math.max(_minX, barRect.x + barRect.width - Theme.panelEdgeInset - width)

                x: Math.max(_minX, Math.min(_preferredX, _maxX))
                y: barRect.y + barRect.height + Theme.popupGap
            }

            Item {
                id: networkPopupAnchor
                x: barRect.x + rightZone.x + statusCapsule.x + statusRow.x + networkWidget.x
                y: barRect.y + rightZone.y + statusCapsule.y + statusRow.y + networkWidget.y
                width: networkWidget.width
                height: networkWidget.height
                visible: false
            }

            NetworkPopup {
                id: networkPopup
                z: 2
                visible: false
                width: Math.min(Theme.networkPanelWidth, barRect.width - Theme.panelEdgeInset * 2)

                readonly property real _preferredX: networkPopupAnchor.x + (networkPopupAnchor.width - width) / 2
                readonly property real _minX: barRect.x + Theme.panelEdgeInset
                readonly property real _maxX: Math.max(_minX, barRect.x + barRect.width - Theme.panelEdgeInset - width)

                x: Math.max(_minX, Math.min(_preferredX, _maxX))
                y: barRect.y + barRect.height + Theme.popupGap
            }

            Item {
                id: volumePopupAnchor
                x: barRect.x + rightZone.x + statusCapsule.x + statusRow.x + volumeWidget.x
                y: barRect.y + rightZone.y + statusCapsule.y + statusRow.y + volumeWidget.y
                width: volumeWidget.width
                height: volumeWidget.height
                visible: false
            }

            VolumePopup {
                id: volumePopup
                z: 2
                visible: false
                width: Math.min(Theme.volumePanelWidth, barRect.width - Theme.panelEdgeInset * 2)

                readonly property real _preferredX: volumePopupAnchor.x + (volumePopupAnchor.width - width) / 2
                readonly property real _minX: barRect.x + Theme.panelEdgeInset
                readonly property real _maxX: Math.max(_minX, barRect.x + barRect.width - Theme.panelEdgeInset - width)

                x: Math.max(_minX, Math.min(_preferredX, _maxX))
                y: barRect.y + barRect.height + Theme.popupGap
            }

            Item {
                id: batteryPopupAnchor
                x: barRect.x + rightZone.x + statusCapsule.x + statusRow.x + batteryWidget.x
                y: barRect.y + rightZone.y + statusCapsule.y + statusRow.y + batteryWidget.y
                width: batteryWidget.width
                height: batteryWidget.height
                visible: false
            }

            BatteryPopup {
                id: batteryPopup
                z: 2
                visible: false
                width: Math.min(Theme.batteryPanelWidth, barRect.width - Theme.panelEdgeInset * 2)

                readonly property real _preferredX: batteryPopupAnchor.x + (batteryPopupAnchor.width - width) / 2
                readonly property real _minX: barRect.x + Theme.panelEdgeInset
                readonly property real _maxX: Math.max(_minX, barRect.x + barRect.width - Theme.panelEdgeInset - width)

                x: Math.max(_minX, Math.min(_preferredX, _maxX))
                y: barRect.y + barRect.height + Theme.popupGap
            }
        }
    }
}
