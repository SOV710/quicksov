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
            // Render a top bar on every output. Screen roles remain available through
            // Meta for any future per-screen behavior, but no longer gate MainBar itself.
            visible: true

            anchors.top:   true
            anchors.left:  true
            anchors.right: true

            margins {
                top:   Theme.barOuterMargin
                left:  Theme.barOuterMargin
                right: Theme.barOuterMargin
            }

            readonly property bool _anyPopupOpen: clockPopup.popupVisible
                                                 || notifCenter.visible
                                                 || bluetoothPopup.visible
                                                 || networkPopup.visible
                                                 || volumePopup.visible
                                                 || batteryPopup.visible

            // Expand to cover bar + whichever popup is open (tallest wins)
            property int _popupHeight: {
                var h = 0;
                if (clockPopup.popupVisible) h = Math.max(h, clockPopup.implicitHeight + Theme.spaceXs);
                if (notifCenter.visible)     h = Math.max(h, notifCenter.implicitHeight + Theme.spaceXs);
                if (bluetoothPopup.visible)  h = Math.max(h, bluetoothPopup.implicitHeight + Theme.spaceXs);
                if (networkPopup.visible)    h = Math.max(h, networkPopup.implicitHeight + Theme.spaceXs);
                if (volumePopup.visible)     h = Math.max(h, volumePopup.implicitHeight + Theme.spaceXs);
                if (batteryPopup.visible)    h = Math.max(h, batteryPopup.implicitHeight + Theme.spaceXs);
                return h;
            }
            implicitHeight: bar._anyPopupOpen && bar.screen
                            ? Math.max(
                                  Theme.barHeight + Theme.barOuterMargin + _popupHeight,
                                  bar.screen.height - Theme.barOuterMargin
                              )
                            : Theme.barHeight + Theme.barOuterMargin + _popupHeight
            // Reserve a fixed bar-height strip only; popups must NOT push windows.
            exclusiveZone: Theme.barHeight
            color: "transparent"

            MouseArea {
                anchors.fill: parent
                visible: bar._anyPopupOpen
                acceptedButtons: Qt.AllButtons
                onClicked: function() {
                    clockPopup.popupVisible = false;
                    notifCenter.visible = false;
                    bluetoothPopup.visible = false;
                    networkPopup.visible = false;
                    volumePopup.visible = false;
                    batteryPopup.visible = false;
                }
            }

            Rectangle {
                id: barRect
                z: 1
                anchors {
                    left:   parent.left
                    right:  parent.right
                    top:    parent.top
                }
                height: Theme.barHeight
                radius: Theme.barRadius
                color:  Theme.bgSurface
                opacity: Theme.opacityPanel
                border.color: Theme.borderDefault
                border.width: 1

                // LEFT zone
                Row {
                    id: leftZone
                    anchors {
                        left:           parent.left
                        leftMargin:     Theme.barPadX
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

                // CENTER zone — absolutely centered
                    Clock {
                        id: clockWidget
                        anchors.centerIn: parent
                        onOpenPopup: {
                            notifCenter.visible = false;
                            bluetoothPopup.visible = false;
                            networkPopup.visible = false;
                            volumePopup.visible = false;
                            batteryPopup.visible = false;
                            clockPopup.popupVisible = !clockPopup.popupVisible;
                        }
                    }

                // RIGHT zone
                Row {
                    id: rightZone
                    anchors {
                        right:          parent.right
                        rightMargin:    Theme.barPadX
                        verticalCenter: parent.verticalCenter
                    }
                    spacing: Theme.spaceSm
                    layoutDirection: Qt.RightToLeft

                    NotificationButton {
                        anchors.verticalCenter: parent.verticalCenter
                        onToggled: {
                            clockPopup.popupVisible = false;
                            bluetoothPopup.visible = false;
                            networkPopup.visible = false;
                            volumePopup.visible = false;
                            batteryPopup.visible = false;
                            notifCenter.visible = !notifCenter.visible;
                        }
                    }

                    VolumeIndicator {
                        id: volumeWidget
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: {
                            clockPopup.popupVisible = false;
                            notifCenter.visible = false;
                            bluetoothPopup.visible = false;
                            networkPopup.visible = false;
                            batteryPopup.visible = false;
                            volumePopup.visible = !volumePopup.visible;
                        }
                    }
                    BluetoothIndicator {
                        id: bluetoothWidget
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: {
                            clockPopup.popupVisible = false;
                            notifCenter.visible = false;
                            networkPopup.visible = false;
                            volumePopup.visible = false;
                            batteryPopup.visible = false;
                            bluetoothPopup.visible = !bluetoothPopup.visible;
                        }
                    }
                    NetworkIndicator {
                        id: networkWidget
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: {
                            clockPopup.popupVisible = false;
                            notifCenter.visible = false;
                            bluetoothPopup.visible = false;
                            volumePopup.visible = false;
                            batteryPopup.visible = false;
                            networkPopup.visible = !networkPopup.visible;
                        }
                    }
                    BatteryIndicator {
                        id: batteryWidget
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: {
                            clockPopup.popupVisible = false;
                            notifCenter.visible = false;
                            bluetoothPopup.visible = false;
                            networkPopup.visible = false;
                            volumePopup.visible = false;
                            batteryPopup.visible = !batteryPopup.visible;
                        }
                    }
                    TrayHost          { anchors.verticalCenter: parent.verticalCenter }
                }
            }

            // Overlays anchored below bar
            ClockPopup {
                id: clockPopup
                z: 2
                availableWidth: barRect.width - Theme.spaceXl * 2
                availableHeight: bar.screen
                                 ? (bar.screen.height - Theme.barHeight - Theme.barOuterMargin * 2 - Theme.spaceXl * 2)
                                 : Theme.clockPanelMaxHeight
                anchors {
                    top:              barRect.bottom
                    topMargin:        Theme.spaceXs
                    horizontalCenter: barRect.horizontalCenter
                }
            }

            NotificationCenter {
                id: notifCenter
                z: 2
                visible: false
                anchors {
                    top:        barRect.bottom
                    topMargin:  Theme.spaceXs
                    right:      barRect.right
                    rightMargin: Theme.barPadX
                }
            }

            Item {
                id: bluetoothPopupAnchor
                x: barRect.x + rightZone.x + bluetoothWidget.x
                y: barRect.y + rightZone.y + bluetoothWidget.y
                width: bluetoothWidget ? bluetoothWidget.width : 0
                height: bluetoothWidget ? bluetoothWidget.height : 0
                visible: false
            }

            BluetoothPopup {
                id: bluetoothPopup
                z: 2
                visible: false
                readonly property real _preferredX: bluetoothPopupAnchor.x + (bluetoothPopupAnchor.width - width) / 2
                readonly property real _minX: barRect.x + Theme.barPadX
                readonly property real _maxX: Math.max(_minX, barRect.x + barRect.width - Theme.barPadX - width)

                x: Math.max(_minX, Math.min(_preferredX, _maxX))
                y: barRect.y + barRect.height + Theme.spaceXs
            }

            Item {
                id: networkPopupAnchor
                x: barRect.x + rightZone.x + networkWidget.x
                y: barRect.y + rightZone.y + networkWidget.y
                width: networkWidget ? networkWidget.width : 0
                height: networkWidget ? networkWidget.height : 0
                visible: false
            }

            NetworkPopup {
                id: networkPopup
                z: 2
                visible: false
                readonly property real _preferredX: networkPopupAnchor.x + (networkPopupAnchor.width - width) / 2
                readonly property real _minX: barRect.x + Theme.barPadX
                readonly property real _maxX: Math.max(_minX, barRect.x + barRect.width - Theme.barPadX - width)

                x: Math.max(_minX, Math.min(_preferredX, _maxX))
                y: barRect.y + barRect.height + Theme.spaceXs
            }

            Item {
                id: volumePopupAnchor
                // Avoid mapToItem() here: the JS call does not reliably track the
                // Row's post-layout geometry updates, which can leave the popup
                // anchored near x=0. Bind directly to the observable layout chain.
                x: barRect.x + rightZone.x + volumeWidget.x
                y: barRect.y + rightZone.y + volumeWidget.y
                width: volumeWidget ? volumeWidget.width : 0
                height: volumeWidget ? volumeWidget.height : 0
                visible: false
            }

            VolumePopup {
                id: volumePopup
                z: 2
                visible: false
                readonly property real _preferredX: volumePopupAnchor.x + (volumePopupAnchor.width - width) / 2
                readonly property real _minX: barRect.x + Theme.barPadX
                readonly property real _maxX: Math.max(_minX, barRect.x + barRect.width - Theme.barPadX - width)

                x: Math.max(_minX, Math.min(_preferredX, _maxX))
                y: barRect.y + barRect.height + Theme.spaceXs
            }

            Item {
                id: batteryPopupAnchor
                x: barRect.x + rightZone.x + batteryWidget.x
                y: barRect.y + rightZone.y + batteryWidget.y
                width: batteryWidget ? batteryWidget.width : 0
                height: batteryWidget ? batteryWidget.height : 0
                visible: false
            }

            BatteryPopup {
                id: batteryPopup
                z: 2
                visible: false
                readonly property real _preferredX: batteryPopupAnchor.x + (batteryPopupAnchor.width - width) / 2
                readonly property real _minX: barRect.x + Theme.barPadX
                readonly property real _maxX: Math.max(_minX, barRect.x + barRect.width - Theme.barPadX - width)

                x: Math.max(_minX, Math.min(_preferredX, _maxX))
                y: barRect.y + barRect.height + Theme.spaceXs
            }
        }
    }
}
