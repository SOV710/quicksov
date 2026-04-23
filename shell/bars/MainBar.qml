// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import Quickshell.Wayland
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
                                                 || statusDock.panelOverflowHeight > 0

            property int _popupHeight: {
                var h = 0;
                if (clockPopup.popupVisible) h = Math.max(h, clockPopup.implicitHeight + Theme.popupGap);
                if (statusDock.panelOverflowHeight > 0)
                    h = Math.max(h, statusDock.panelOverflowHeight);
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
            mask: barSurfaceRegion

            BackgroundEffect.blurRegion: barSurfaceRegion

            Region {
                id: barSurfaceRegion
                item: barRect
                radius: barRect.radius

                Region {
                    item: clockPopup.popupVisible ? clockPopup.shellItem : null
                    radius: clockPopup.shellRadius
                }

                Region {
                    item: statusDock.shellVisible ? statusDock.blurBodyItem : null
                    topLeftRadius: 0
                    topRightRadius: 0
                    bottomLeftRadius: Theme.statusDockLowerRadius
                    bottomRightRadius: Theme.statusDockLowerRadius

                    Region {
                        item: statusDock.shellVisible ? statusDock.blurLeftCornerSquareItem : null
                        intersection: Intersection.Combine

                        Region {
                            item: statusDock.shellVisible ? statusDock.blurLeftShoulderArcItem : null
                            shape: RegionShape.Ellipse
                            intersection: Intersection.Intersect
                        }
                    }

                    Region {
                        item: statusDock.shellVisible ? statusDock.blurRightCornerSquareItem : null
                        intersection: Intersection.Combine

                        Region {
                            item: statusDock.shellVisible ? statusDock.blurRightShoulderArcItem : null
                            shape: RegionShape.Ellipse
                            intersection: Intersection.Intersect
                        }
                    }
                }
            }

            function closeAllPopups() {
                clockPopup.popupVisible = false;
                statusDock.close();
            }

            MouseArea {
                anchors.fill: parent
                visible: bar._anyPopupOpen
                acceptedButtons: Qt.AllButtons
                onClicked: bar.closeAllPopups()
            }

            Rectangle {
                id: barShadow
                z: 0
                x: barRect.x
                y: barRect.y + 3
                width: barRect.width
                height: barRect.height
                radius: barRect.radius + 1
                color: Theme.barShadowColor
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
                color: Theme.barShellFill
                border.color: Theme.barShellBorder
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
                                anchors.verticalCenter: parent.verticalCenter
                                onClicked: statusDock.togglePanel("battery")
                            }

                            NetworkIndicator {
                                anchors.verticalCenter: parent.verticalCenter
                                onClicked: statusDock.togglePanel("network")
                            }

                            BluetoothIndicator {
                                anchors.verticalCenter: parent.verticalCenter
                                onClicked: statusDock.togglePanel("bluetooth")
                            }

                            VolumeIndicator {
                                anchors.verticalCenter: parent.verticalCenter
                                onClicked: statusDock.togglePanel("volume")
                            }

                            NotificationButton {
                                anchors.verticalCenter: parent.verticalCenter
                                onToggled: statusDock.togglePanel("notification")
                            }
                        }
                    }
                }
            }

            StatusDockHost {
                id: statusDock
                barItem: barRect
                triggerItem: statusCapsule
                availableWidth: Math.max(0, barRect.width - Theme.panelEdgeInset * 2)
                maxPanelHeight: bar.screen
                                ? Math.max(
                                      0,
                                      Math.min(
                                          Theme.rightPopupMaxHeight,
                                          bar.screen.height - Theme.barHeight - Theme.barOuterMargin * 2 - Theme.panelEdgeInset
                                      )
                                  )
                                : Theme.rightPopupMaxHeight
                onToggleRequested: clockPopup.popupVisible = false
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

        }
    }
}
