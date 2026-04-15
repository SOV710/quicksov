// SPDX-FileCopyrightText: 2026 SOV710
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
            // Bootstrap fallback: show on any screen until Meta has resolved roles.
            // Once Meta.ready, only the screen with role "main" shows this bar.
            visible: !Meta.ready || Meta.screenRoles[modelData.name] === "main"

            anchors.top:   true
            anchors.left:  true
            anchors.right: true

            margins {
                top:   Theme.barOuterMargin
                left:  Theme.barOuterMargin
                right: Theme.barOuterMargin
            }

            implicitHeight: Theme.barHeight + Theme.barOuterMargin
            color: "transparent"

            Rectangle {
                id: barRect
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
                    spacing: Theme.spaceSm

                    WorkspaceStrip {
                        outputName: bar.screen ? bar.screen.name : ""
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    WindowInfo {
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                // CENTER zone — absolutely centered
                Clock {
                    id: clockWidget
                    anchors.centerIn: parent
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
                        onToggled: notifCenter.visible = !notifCenter.visible
                    }

                    VolumeIndicator   { anchors.verticalCenter: parent.verticalCenter }
                    BluetoothIndicator{ anchors.verticalCenter: parent.verticalCenter }
                    NetworkIndicator  { anchors.verticalCenter: parent.verticalCenter }
                    BatteryIndicator  { anchors.verticalCenter: parent.verticalCenter }
                    TrayHost          { anchors.verticalCenter: parent.verticalCenter }
                }
            }

            // Overlays anchored below bar
            ClockPopup {
                id: clockPopup
                anchors {
                    top:              barRect.bottom
                    topMargin:        Theme.spaceXs
                    horizontalCenter: barRect.horizontalCenter
                }
            }

            NotificationCenter {
                id: notifCenter
                visible: false
                anchors {
                    top:        barRect.bottom
                    topMargin:  Theme.spaceXs
                    right:      barRect.right
                    rightMargin: Theme.barPadX
                }
            }
        }
    }
}
