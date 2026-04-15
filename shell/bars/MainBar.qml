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
            // Show on every screen when Meta is not ready or no roles are configured.
            // Once Meta has role data, only the "main" screen shows this bar.
            visible: !Meta.ready || !Meta.hasScreenRoles || Meta.screenRoles[modelData.name] === "main"

            anchors.top:   true
            anchors.left:  true
            anchors.right: true

            margins {
                top:   Theme.barOuterMargin
                left:  Theme.barOuterMargin
                right: Theme.barOuterMargin
            }

            // Expand to cover bar + whichever popup is open (tallest wins)
            property int _popupHeight: {
                var h = 0;
                if (clockPopup.popupVisible) h = Math.max(h, 180 + Theme.spaceXs);
                if (notifCenter.visible)     h = Math.max(h, notifCenter.implicitHeight + Theme.spaceXs);
                return h;
            }
            implicitHeight: Theme.barHeight + Theme.barOuterMargin + _popupHeight
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
                    onOpenPopup: clockPopup.popupVisible = !clockPopup.popupVisible
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
