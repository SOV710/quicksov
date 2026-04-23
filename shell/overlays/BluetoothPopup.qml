// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Layouts
import ".."
import "../components"
import "../services"

Rectangle {
    id: root

    readonly property Item shellItem: root
    readonly property int shellRadius: Theme.radiusXl

    width: Theme.bluetoothPanelWidth
    implicitHeight: height
    height: Math.min(contentCol.implicitHeight + Theme.spaceMd * 2, Theme.bluetoothPanelMaxHeight)
    radius: Theme.radiusXl
    color: Theme.popupShellFill
    border.color: Theme.popupShellBorder
    border.width: 1
    opacity: visible ? 1 : 0
    clip: true

    readonly property bool _showLoadingState: !Bluetooth.ready
    readonly property bool _showUnavailableState: Bluetooth.ready && !Bluetooth.btAvailable
    readonly property bool _showDisabledState: Bluetooth.ready && Bluetooth.btAvailable && !Bluetooth.btEnabled
    readonly property bool _showEmptyState: Bluetooth.ready
                                            && Bluetooth.btAvailable
                                            && Bluetooth.btEnabled
                                            && Bluetooth.devices.length === 0
    readonly property bool _showDeviceList: Bluetooth.ready
                                            && Bluetooth.btAvailable
                                            && Bluetooth.btEnabled
                                            && Bluetooth.devices.length > 0
    readonly property real _listMaxHeight: Math.max(
        Theme.spaceXxl * 3,
        Theme.bluetoothPanelMaxHeight - headerRow.implicitHeight - Theme.spaceMd * 4
    )

    function _subtitle() {
        if (!Bluetooth.ready) return "Waiting for daemon";
        if (!Bluetooth.btAvailable) return "No Bluetooth adapter";
        if (!Bluetooth.btEnabled) return "Bluetooth is off";

        var connected = Bluetooth.connectedDevices.length;
        if (Bluetooth.discovering) {
            return connected > 0 ? String(connected) + " connected • scanning" : "Scanning nearby devices";
        }

        if (connected > 0) {
            return connected === 1 ? "1 connected device" : String(connected) + " connected devices";
        }

        if (Bluetooth.devices.length > 0) {
            return String(Bluetooth.devices.length) + " known devices";
        }

        return "Ready";
    }

    function _actionLabel(device) {
        if (!device) return "";
        if (device.connected) return "Disconnect";
        if (device.paired) return "Connect";
        return "Pair";
    }

    function _runPrimaryAction(device) {
        if (!device || !device.address) return;

        if (device.connected) {
            Bluetooth.disconnectDevice(device.address);
        } else if (device.paired) {
            Bluetooth.connectDevice(device.address);
        } else {
            Bluetooth.pairDevice(device.address);
        }
    }

    Behavior on opacity { NumberAnimation { duration: Theme.motionFast } }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onClicked: function(mouse) { mouse.accepted = true; }
        onPressed: function(mouse) { mouse.accepted = true; }
    }

    Column {
        id: contentCol
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            margins: Theme.spaceMd
        }
        spacing: Theme.spaceMd

        RowLayout {
            id: headerRow
            width: parent.width
            spacing: Theme.spaceSm

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2

                Text {
                    text: "Bluetooth"
                    color: Theme.fgPrimary
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontLabel
                    font.weight: Theme.weightSemibold
                    Layout.fillWidth: true
                }

                Text {
                    text: root._subtitle()
                    color: Theme.fgMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }

            HeaderChip {
                label: Bluetooth.scanPending
                       ? (Bluetooth.discovering ? "Stopping" : "Starting")
                       : Bluetooth.scanBlocked
                         ? "Paused"
                       : (Bluetooth.discovering ? "Stop" : "Refresh")
                iconPath: Bluetooth.discovering ? "lucide/loader-circle.svg" : "lucide/rotate-cw.svg"
                enabled: Bluetooth.ready && Bluetooth.btAvailable && Bluetooth.btEnabled && !Bluetooth.scanBlocked
                active: Bluetooth.discovering || Bluetooth.scanPending
                pending: Bluetooth.scanPending
                spinning: Bluetooth.discovering
                onClicked: Bluetooth.toggleScan()
            }

            HeaderChip {
                label: Bluetooth.powerPending
                       ? (Bluetooth.btEnabled ? "Turning off" : "Turning on")
                       : (Bluetooth.btEnabled ? "On" : "Off")
                enabled: Bluetooth.ready && Bluetooth.btAvailable
                active: Bluetooth.btEnabled || Bluetooth.powerPending
                pending: Bluetooth.powerPending
                onClicked: Bluetooth.togglePowered()
            }
        }

        Text {
            visible: Bluetooth.lastError !== ""
            width: parent.width
            text: Bluetooth.lastError
            color: Theme.colorError
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            wrapMode: Text.WordWrap
        }

        Text {
            visible: Bluetooth.scanBlockedReason !== ""
            width: parent.width
            text: Bluetooth.scanBlockedReason
            color: Theme.fgMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            wrapMode: Text.WordWrap
        }

        StateCard {
            visible: root._showLoadingState
            iconPath: "lucide/loader-circle.svg"
            title: "Waiting for daemon"
            message: "Bluetooth status is not available yet."
            spinning: true
        }

        StateCard {
            visible: root._showUnavailableState
            iconPath: Theme.iconBluetoothOffStatus
            title: "No Bluetooth adapter"
            message: "Connect an adapter or start BlueZ, then reopen this panel."
        }

        StateCard {
            visible: root._showDisabledState
            iconPath: Theme.iconBluetoothOffStatus
            title: "Bluetooth is off"
            message: "Turn Bluetooth on to view paired devices and scan nearby devices."
        }

        StateCard {
            visible: root._showEmptyState
            iconPath: Bluetooth.discovering ? "lucide/loader-circle.svg" : Theme.iconBluetoothStatus
            title: Bluetooth.discovering ? "Scanning for devices" : "No devices"
            message: Bluetooth.discovering
                     ? "Nearby devices will appear here."
                     : "Use Refresh to scan nearby devices."
            spinning: Bluetooth.discovering
        }

        Flickable {
            id: listArea
            visible: root._showDeviceList
            width: parent.width
            height: Math.min(devicesCol.implicitHeight, root._listMaxHeight)
            contentHeight: devicesCol.implicitHeight
            contentWidth: width
            clip: true
            interactive: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: devicesCol
                width: listArea.width
                spacing: Theme.spaceXs

                SectionLabel {
                    visible: Bluetooth.connectedDevices.length > 0
                    text: "Connected"
                }

                Repeater {
                    model: Bluetooth.connectedDevices

                    delegate: DeviceCard {
                        required property var modelData
                        width: devicesCol.width
                        device: modelData
                    }
                }

                SectionLabel {
                    visible: Bluetooth.pairedDevices.length > 0
                    text: "Paired"
                }

                Repeater {
                    model: Bluetooth.pairedDevices

                    delegate: DeviceCard {
                        required property var modelData
                        width: devicesCol.width
                        device: modelData
                    }
                }

                SectionLabel {
                    visible: Bluetooth.availableDevices.length > 0
                    text: "Available"
                }

                Repeater {
                    model: Bluetooth.availableDevices

                    delegate: DeviceCard {
                        required property var modelData
                        width: devicesCol.width
                        device: modelData
                    }
                }
            }
        }
    }

    component HeaderChip: Rectangle {
        id: chip

        property string label: ""
        property string iconPath: ""
        property bool enabled: true
        property bool active: false
        property bool pending: false
        property bool spinning: false

        signal clicked()

        implicitWidth: chipRow.implicitWidth + Theme.spaceSm * 2
        implicitHeight: Theme.barHeight - Theme.spaceXs
        radius: Theme.radiusSm
        color: chipMouse.pressed
               ? Theme.surfaceActive
               : (active || pending)
                 ? Theme.surfaceActive
                 : chipMouse.containsMouse
                   ? Theme.surfaceHover
                   : Theme.bgSurfaceRaised
        border.color: (active || pending) ? Theme.borderAccent : Theme.borderSubtle
        border.width: 1
        opacity: enabled ? 1.0 : 0.45
        scale: chipMouse.pressed ? 0.98 : 1.0

        Behavior on color { ColorAnimation { duration: Theme.motionFast } }
        Behavior on scale { NumberAnimation { duration: Theme.motionFast } }

        Row {
            id: chipRow
            anchors.centerIn: parent
            spacing: Theme.spaceXs

            SvgIcon {
                id: chipIcon
                visible: chip.pending || chip.iconPath !== ""
                iconPath: chip.pending ? "lucide/loader-circle.svg" : chip.iconPath
                size: Theme.iconSize - 2
                color: (chip.active || chip.pending) ? Theme.accentBlue : Theme.fgSecondary

                RotationAnimator on rotation {
                    running: chip.pending || chip.spinning
                    from: 0
                    to: 360
                    duration: 1000
                    loops: Animation.Infinite
                }
            }

            Text {
                text: chip.label
                color: (chip.active || chip.pending) ? Theme.fgPrimary : Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Theme.weightMedium
            }
        }

        MouseArea {
            id: chipMouse
            anchors.fill: parent
            hoverEnabled: true
            enabled: chip.enabled
            cursorShape: chip.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: chip.clicked()
        }
    }

    component SectionLabel: Text {
        color: Theme.fgMuted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.weight: Theme.weightMedium
        topPadding: Theme.spaceXs
        bottomPadding: Theme.spaceXs
    }

    component StateCard: Rectangle {
        id: stateCard

        property string iconPath: ""
        property string title: ""
        property string message: ""
        property bool spinning: false

        width: contentCol.width
        radius: Theme.radiusSm
        color: Theme.bgSurfaceRaised
        border.color: Theme.borderSubtle
        border.width: 1
        implicitHeight: stateCol.implicitHeight + Theme.spaceLg * 2

        Column {
            id: stateCol
            anchors.centerIn: parent
            width: parent.width - Theme.spaceLg * 2
            spacing: Theme.spaceSm

            SvgIcon {
                id: stateIcon
                anchors.horizontalCenter: parent.horizontalCenter
                iconPath: stateCard.iconPath
                size: Theme.iconSize + Theme.spaceMd
                color: Theme.fgMuted

                RotationAnimator on rotation {
                    running: stateCard.spinning
                    from: 0
                    to: 360
                    duration: 1000
                    loops: Animation.Infinite
                }
            }

            Text {
                text: stateCard.title
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Theme.weightMedium
            }

            Text {
                text: stateCard.message
                width: parent.width
                wrapMode: Text.WordWrap
                horizontalAlignment: Text.AlignHCenter
                color: Theme.fgMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
        }
    }

    component DeviceCard: Rectangle {
        id: card

        required property var device

        readonly property string _label: Bluetooth.deviceLabel(device)
        readonly property string _status: Bluetooth.deviceStatus(device)
        readonly property bool _hasBattery: device && device.battery !== null && device.battery !== undefined
        readonly property bool _pending: device && Bluetooth.devicePending(device.address)
        readonly property string _pendingAction: device ? Bluetooth.devicePendingAction(device.address) : ""

        radius: Theme.radiusSm
        color: cardHover.containsMouse ? Theme.surfaceHover : Theme.bgSurfaceRaised
        border.color: device && device.connected ? Theme.borderAccent : Theme.borderSubtle
        border.width: 1
        implicitHeight: cardRow.implicitHeight + Theme.spaceSm * 2

        HoverHandler { id: cardHover }

        Behavior on color { ColorAnimation { duration: Theme.motionFast } }

        RowLayout {
            id: cardRow
            anchors.fill: parent
            anchors.margins: Theme.spaceSm
            spacing: Theme.spaceSm

            Rectangle {
                Layout.preferredWidth: Theme.iconSize + Theme.spaceSm
                Layout.preferredHeight: Theme.iconSize + Theme.spaceSm
                radius: Theme.radiusXs
                color: device && device.connected ? Theme.surfaceActive : Theme.bgSurface

                SvgIcon {
                    anchors.centerIn: parent
                    iconPath: Theme.iconBluetoothStatus
                    size: Theme.iconSize
                    color: device && device.connected ? Theme.accentBlue : Theme.fgSecondary
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2

                Text {
                    text: card._label
                    color: Theme.fgPrimary
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    font.weight: Theme.weightMedium
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }

                Text {
                    text: card._status
                    color: Theme.fgMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }

            Text {
                visible: card._hasBattery
                text: String(device.battery) + "%"
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                font.features: { "tnum": 1 }
            }

            HeaderChip {
                label: card._pending
                       ? card._pendingAction
                       : root._actionLabel(card.device)
                active: card.device && card.device.connected
                enabled: !card._pending
                pending: card._pending
                onClicked: root._runPrimaryAction(card.device)
            }
        }
    }
}
