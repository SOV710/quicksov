// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Layouts
import ".."
import "../components"
import "../services"

Item {
    id: root

    width: parent ? parent.width : Theme.bluetoothPanelWidth
    implicitHeight: Math.min(contentCol.implicitHeight + Theme.spaceMd * 2, Theme.bluetoothPanelMaxHeight)

    readonly property int _visibleDeviceCount: Bluetooth.visibleDeviceCount
    readonly property bool _showLoadingState: !Bluetooth.ready
    readonly property bool _showUnavailableState: Bluetooth.ready && !Bluetooth.btAvailable
    readonly property bool _showDisabledState: Bluetooth.ready && Bluetooth.btAvailable && !Bluetooth.btEnabled
    readonly property bool _showEmptyState: Bluetooth.ready
                                            && Bluetooth.btAvailable
                                            && Bluetooth.btEnabled
                                            && root._visibleDeviceCount === 0
    readonly property bool _showDeviceList: Bluetooth.ready
                                            && Bluetooth.btAvailable
                                            && Bluetooth.btEnabled
                                            && root._visibleDeviceCount > 0
    readonly property real _listMaxHeight: Math.max(
        Theme.spaceXxl * 3,
        Theme.bluetoothPanelMaxHeight - headerRow.implicitHeight - Theme.spaceMd * 5
    )

    function _subtitle() {
        if (!Bluetooth.ready) return "Waiting for daemon";
        if (!Bluetooth.btAvailable) return "No Bluetooth adapter";
        if (!Bluetooth.btEnabled) return "Bluetooth is off";

        var connected = Bluetooth.connectedDevices.length;
        if (Bluetooth.discovering)
            return connected > 0 ? String(connected) + " connected, scanning" : "Scanning nearby devices";

        if (connected > 0)
            return connected === 1 ? "1 connected device" : String(connected) + " connected devices";

        if (Bluetooth.visibleDeviceCount > 0)
            return String(Bluetooth.visibleDeviceCount) + " devices available";

        return "Ready";
    }

    function _deviceIconPath(device) {
        if (!device)
            return Theme.iconBluetoothStatus;

        var iconName = device.icon ? String(device.icon).toLowerCase() : "";
        if (iconName.indexOf("audio") >= 0
                || iconName.indexOf("headphone") >= 0
                || iconName.indexOf("headset") >= 0)
            return Theme.iconHeadphonesStatus;

        return Theme.iconBluetoothStatus;
    }

    function _deviceDetailText(device) {
        if (!device)
            return "";

        var parts = [];
        if (device.battery !== null && device.battery !== undefined)
            parts.push(String(device.battery) + "%");

        return parts.join(" ");
    }

    function _runPrimaryAction(device) {
        if (!device || !device.address)
            return;

        if (device.connected) {
            Bluetooth.disconnectDevice(device.address);
        } else if (device.paired) {
            Bluetooth.connectDevice(device.address);
        } else {
            Bluetooth.pairDevice(device.address);
        }
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
                    font.pixelSize: Theme.fontDisplay
                    font.weight: Theme.weightSemibold
                    Layout.fillWidth: true
                }

                Text {
                    text: root._subtitle()
                    color: Theme.fgMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }

            RowLayout {
                spacing: Theme.spaceXs

                IconCircleButton {
                    iconPath: Theme.iconRefreshStatus
                    enabled: Bluetooth.ready
                             && Bluetooth.btAvailable
                             && Bluetooth.btEnabled
                             && !Bluetooth.scanBlocked
                             && !Bluetooth.scanPending
                    active: Bluetooth.discovering || Bluetooth.scanPending
                    spinning: Bluetooth.discovering || Bluetooth.scanPending
                    onClicked: Bluetooth.toggleScan()
                }

                IconCircleButton {
                    iconPath: Theme.iconPowerStatus
                    enabled: Bluetooth.ready && Bluetooth.btAvailable
                    active: Bluetooth.btEnabled || Bluetooth.powerPending
                    spinning: Bluetooth.powerPending
                    onClicked: Bluetooth.togglePowered()
                }
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
            font.pixelSize: Theme.fontSmall
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
                     : "Use refresh to scan nearby devices."
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
                    visible: Bluetooth.savedColumnDevices.length > 0
                    width: parent.width
                    text: "Saved"
                }

                Repeater {
                    model: Bluetooth.savedColumnDevices

                    delegate: DeviceCard {
                        required property var modelData
                        width: devicesCol.width
                        device: modelData
                    }
                }

                SectionLabel {
                    visible: Bluetooth.availableColumnDevices.length > 0
                    width: parent.width
                    text: "Available"
                }

                Repeater {
                    model: Bluetooth.availableColumnDevices

                    delegate: DeviceCard {
                        required property var modelData
                        width: devicesCol.width
                        device: modelData
                    }
                }
            }
        }
    }

    component IconCircleButton: Rectangle {
        id: chip

        property string iconPath: ""
        property bool enabled: true
        property bool active: false
        property bool spinning: false
        property bool danger: false
        property real buttonSize: Theme.barHeight

        signal clicked()

        implicitWidth: buttonSize
        implicitHeight: buttonSize
        radius: buttonSize / 2
        readonly property color _activeFill: danger
                                             ? Theme.overlay(Theme.bgSurfaceRaised, Theme.colorError, 0.16)
                                             : Theme.surfaceActive
        readonly property color _activeBorder: danger ? Theme.dangerBorderSoft : Theme.borderAccent
        readonly property color _iconColor: !enabled
                                            ? Theme.fgDisabled
                                            : danger
                                              ? Theme.colorError
                                              : (active || spinning)
                                                ? Theme.accentBlue
                                                : Theme.fgSecondary
        color: chipMouse.pressed
               ? Theme.surfaceActive
               : (active || spinning)
                 ? chip._activeFill
                 : chipMouse.containsMouse
                   ? Theme.surfaceHover
                   : Theme.bgSurfaceRaised
        border.color: (active || spinning)
                      ? chip._activeBorder
                      : danger
                        ? Theme.withAlpha(Theme.colorError, 0.24)
                        : Theme.borderSubtle
        border.width: 1
        opacity: enabled ? 1.0 : 0.45
        scale: chipMouse.pressed ? 0.98 : 1.0

        Behavior on color { ColorAnimation { duration: Theme.motionFast } }
        Behavior on scale { NumberAnimation { duration: Theme.motionFast } }

        SvgIcon {
            anchors.centerIn: parent
            iconPath: chip.iconPath
            size: Theme.iconSize - 2
            color: chip._iconColor

            RotationAnimator on rotation {
                running: chip.spinning
                from: 0
                to: 360
                duration: 1000
                loops: Animation.Infinite
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
        readonly property string _detail: root._deviceDetailText(device)
        readonly property bool _pending: device && Bluetooth.devicePending(device.address)
        readonly property bool _hasAddress: device && device.address !== undefined && String(device.address) !== ""
        readonly property bool _showForget: device && device.paired && !device.connected && !card._pending
        readonly property bool _primaryActionEnabled: card._hasAddress && !card._pending
        readonly property bool _forgetActionEnabled: card._hasAddress && !card._pending
        readonly property bool _showMetaRow: connectedStateIcon.visible
                                             || savedStateIcon.visible
                                             || detailText.text.length > 0
        readonly property real _actionButtonSize: Theme.iconSize + Theme.spaceLg

        radius: Theme.radiusSm
        color: device && device.connected
               ? Theme.overlay(cardHover.containsMouse ? Theme.surfaceHover : Theme.bgSurfaceRaised,
                               Theme.accentBlue,
                               0.14)
               : cardHover.containsMouse
                 ? Theme.surfaceHover
                 : Theme.bgSurfaceRaised
        border.color: device && device.connected ? Theme.borderAccent : Theme.borderSubtle
        border.width: 1
        implicitHeight: cardCol.implicitHeight + Theme.spaceSm * 2

        HoverHandler { id: cardHover }

        Behavior on color { ColorAnimation { duration: Theme.motionFast } }

        ColumnLayout {
            id: cardCol
            anchors.fill: parent
            anchors.margins: Theme.spaceSm
            spacing: Theme.spaceSm

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceSm

                Rectangle {
                    Layout.preferredWidth: Theme.iconSize + Theme.spaceSm
                    Layout.preferredHeight: Theme.iconSize + Theme.spaceSm
                    radius: Theme.radiusXs
                    color: device && device.connected ? Theme.surfaceActive : Theme.bgSurface

                    SvgIcon {
                        anchors.centerIn: parent
                        iconPath: root._deviceIconPath(device)
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

                    RowLayout {
                        visible: card._showMetaRow
                        spacing: Theme.spaceXs
                        Layout.fillWidth: true

                        SvgIcon {
                            id: connectedStateIcon
                            visible: device && device.connected
                            iconPath: Theme.iconRadioButtonCheckedStatus
                            size: Theme.fontSmall + 2
                            color: Theme.accentBlue
                        }

                        SvgIcon {
                            id: savedStateIcon
                            visible: device && !device.connected && device.paired
                            iconPath: Theme.iconBookmarkStatus
                            size: Theme.fontSmall + 2
                            color: Theme.accentBlue
                        }

                        Text {
                            id: detailText
                            visible: text.length > 0
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            text: card._detail
                            color: Theme.fgMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            elide: Text.ElideRight
                            font.features: { "tnum": 1 }
                        }
                    }
                }

                RowLayout {
                    spacing: Theme.spaceXs

                    IconCircleButton {
                        buttonSize: card._actionButtonSize
                        iconPath: card._pending
                                  ? Theme.iconRefreshStatus
                                  : (device && device.connected ? Theme.iconCloseStatus : Theme.iconCheckStatus)
                        enabled: card._pending ? false : card._primaryActionEnabled
                        active: !card._pending
                        spinning: card._pending
                        onClicked: root._runPrimaryAction(device)
                    }

                    IconCircleButton {
                        visible: card._showForget
                        buttonSize: card._actionButtonSize
                        iconPath: Theme.iconDeleteStatus
                        enabled: card._forgetActionEnabled
                        danger: true
                        onClicked: Bluetooth.forgetDevice(device.address)
                    }
                }
            }
        }
    }
}
