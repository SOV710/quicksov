// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import Quickshell.Wayland
import ".."
import "../widgets"
import "../overlays"

PanelWindow {
    id: root

    required property var screenModel
    screen: screenModel
    visible: true

    anchors.top: true
    anchors.left: true
    anchors.right: true
    anchors.bottom: true

    exclusionMode: ExclusionMode.Ignore
    exclusiveZone: 0
    aboveWindows: true
    focusable: false
    color: "transparent"
    mask: popupController.anyOpen ? captureMask : surfaceMask

    BackgroundEffect.blurRegion: blurRegion

    MainBarPopupController {
        id: popupController
    }

    readonly property real _barAvailableWidth: Math.max(0, barRect.width - Theme.panelEdgeInset * 2)
    readonly property real _statusMaxBodyHeight: root.screen
                                                 ? Math.max(
                                                       0,
                                                       Math.min(
                                                           Theme.rightPopupMaxHeight,
                                                           root.screen.height
                                                           - Theme.barHeight
                                                           - Theme.barOuterMargin * 2
                                                           - Theme.panelEdgeInset
                                                       )
                                                   )
                                                 : Theme.rightPopupMaxHeight
    readonly property real _clockMaxBodyHeight: root.screen
                                                ? Math.max(
                                                      0,
                                                      root.screen.height
                                                      - Theme.barHeight
                                                      - Theme.barOuterMargin * 2
                                                      - Theme.panelEdgeInset * 2
                                                  )
                                                : Theme.clockPanelMaxHeight
    readonly property real _clockPreferredWidth: Math.max(
        0,
        Math.min(Theme.clockPanelMaxWidth, _barAvailableWidth)
    )

    function closeAllPopups() {
        popupController.close();
    }

    Item {
        id: windowBounds
        anchors.fill: parent
        visible: false
    }

    Region {
        id: captureMask
        item: windowBounds
    }

    Region {
        id: surfaceMask
        item: barRect
        radius: barRect.radius

        Region {
            item: clockDock.shellVisible ? clockDock : null
            topLeftRadius: clockDock.topLeftRadius
            topRightRadius: clockDock.topRightRadius
            bottomLeftRadius: Theme.statusDockLowerRadius
            bottomRightRadius: Theme.statusDockLowerRadius
        }

        Region {
            item: statusDock.shellVisible ? statusDock : null
            topLeftRadius: statusDock.topLeftRadius
            topRightRadius: statusDock.topRightRadius
            bottomLeftRadius: Theme.statusDockLowerRadius
            bottomRightRadius: Theme.statusDockLowerRadius
        }
    }

    Region {
        id: blurRegion
        item: barRect
        radius: barRect.radius

        Region {
            item: clockDock.shellVisible ? clockDock : null
            topLeftRadius: clockDock.topLeftRadius
            topRightRadius: clockDock.topRightRadius
            bottomLeftRadius: Theme.statusDockLowerRadius
            bottomRightRadius: Theme.statusDockLowerRadius
        }

        Region {
            item: statusDock.shellVisible ? statusDock : null
            topLeftRadius: statusDock.topLeftRadius
            topRightRadius: statusDock.topRightRadius
            bottomLeftRadius: Theme.statusDockLowerRadius
            bottomRightRadius: Theme.statusDockLowerRadius
        }
    }

    MouseArea {
        anchors.fill: parent
        visible: popupController.anyOpen
        acceptedButtons: Qt.AllButtons
        onClicked: root.closeAllPopups()
    }

    Rectangle {
        id: barShadow
        z: 1
        x: Theme.barOuterMargin
        y: Theme.barOuterMargin + 3
        width: root.width - Theme.barOuterMargin * 2
        height: Theme.barHeight
        radius: Theme.barRadius + 1
        color: Theme.barShadowColor
    }

    Rectangle {
        id: barRect
        z: 2
        x: Theme.barOuterMargin
        y: Theme.barOuterMargin
        width: root.width - Theme.barOuterMargin * 2
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
                outputName: root.screen ? root.screen.name : ""
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
            onOpenPopup: popupController.toggle("clock")
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
                        onClicked: popupController.toggle("battery")
                    }

                    NetworkIndicator {
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: popupController.toggle("network")
                    }

                    BluetoothIndicator {
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: popupController.toggle("bluetooth")
                    }

                    VolumeIndicator {
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: popupController.toggle("volume")
                    }

                    NotificationButton {
                        anchors.verticalCenter: parent.verticalCenter
                        onToggled: popupController.toggle("notification")
                    }
                }
            }
        }
    }

    DockedPanelShell {
        id: clockDock
        z: 3
        barItem: barRect
        triggerItem: clockWidget
        alignmentMode: "center"
        preferredWidth: root._clockPreferredWidth
        availableWidth: root._barAvailableWidth
        maxBodyHeight: root._clockMaxBodyHeight
        open: popupController.clockOpen
        fillColor: Theme.barShellFill
        strokeColor: Theme.barShellBorder
        contentComponent: Component {
            ClockPopup {
                width: parent ? parent.width : 0
                height: parent ? parent.height : implicitHeight
            }
        }
    }

    DockedPanelShell {
        id: statusDock
        z: 3
        barItem: barRect
        triggerItem: statusCapsule
        alignmentMode: "right"
        preferredWidth: Theme.rightPopupWidth
        availableWidth: root._barAvailableWidth
        maxBodyHeight: root._statusMaxBodyHeight
        open: popupController.statusPopup !== ""
        fillColor: Theme.barShellFill
        strokeColor: Theme.barShellBorder
        contentComponent: popupController.statusPopup === "battery"
                          ? batteryPopupComponent
                          : popupController.statusPopup === "network"
                            ? networkPopupComponent
                            : popupController.statusPopup === "bluetooth"
                              ? bluetoothPopupComponent
                              : popupController.statusPopup === "volume"
                                ? volumePopupComponent
                                : popupController.statusPopup === "notification"
                                  ? notificationPopupComponent
                                  : null
    }

    Component {
        id: batteryPopupComponent
        BatteryPopup {
            width: parent ? parent.width : Theme.batteryPanelWidth
        }
    }

    Component {
        id: networkPopupComponent
        NetworkPopup {
            width: parent ? parent.width : Theme.networkPanelWidth
        }
    }

    Component {
        id: bluetoothPopupComponent
        BluetoothPopup {
            width: parent ? parent.width : Theme.bluetoothPanelWidth
        }
    }

    Component {
        id: volumePopupComponent
        VolumePopup {
            width: parent ? parent.width : Theme.volumePanelWidth
        }
    }

    Component {
        id: notificationPopupComponent
        NotificationCenter {
            width: parent ? parent.width : Theme.notificationPanelWidth
        }
    }
}
