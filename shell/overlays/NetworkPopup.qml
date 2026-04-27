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

    width: parent ? parent.width : Theme.networkPanelWidth
    implicitHeight: Math.min(contentCol.implicitHeight + Theme.spaceMd * 2, Theme.networkPanelMaxHeight)

    property string expandedSsid: ""
    property string passwordText: ""

    readonly property bool _showLoadingState: !Network.ready
    readonly property bool _showUnavailableState: Network.ready && Network.isUnavailable
    readonly property bool _showDisabledState: Network.ready && Network.isDisabled
    readonly property bool _showEmptyState: Network.ready
                                         && Network.availability === "ready"
                                         && Network.networks.length === 0
                                         && !Network.scanning
    readonly property bool _showScanningState: Network.ready
                                             && Network.availability === "ready"
                                             && Network.networks.length === 0
                                             && Network.scanning
    readonly property bool _showList: Network.ready
                                    && Network.availability === "ready"
                                    && Network.networks.length > 0
    readonly property real _listMaxHeight: Math.max(
        Theme.spaceXxl * 3,
        Theme.networkPanelMaxHeight - headerRow.implicitHeight - Theme.spaceMd * 5
    )

    function _subtitle() {
        return Network.subtitle();
    }

    function _availabilityIcon() {
        switch (Network.availabilityReason) {
        case "permission_denied":
            return "lucide/triangle-alert.svg";
        case "backend_error":
            return "lucide/triangle-alert.svg";
        default:
            return Theme.iconWifiZeroStatus;
        }
    }

    function _networkSubtitle(network) {
        if (!network)
            return "";

        var parts = [];
        if (network.current)
            parts.push("Connected");
        else if (network.saved)
            parts.push("Saved");
        else
            parts.push(network.secure ? network.securityLabel : "Open");

        if (network.current && Network.currentIpv4 !== "")
            parts.push(Network.currentIpv4);
        if (!network.current && network.secure)
            parts.push(network.securityLabel);
        if (network.bandLabel)
            parts.push(network.bandLabel);
        if (network.signalPct >= 0)
            parts.push(String(network.signalPct) + "%");

        return parts.join(" • ");
    }

    function _expandPassword(network) {
        root.expandedSsid = network ? network.ssid : "";
        root.passwordText = "";
    }

    function _connectNetwork(network) {
        if (!network)
            return;

        if (network.current) {
            Network.disconnectCurrent();
            return;
        }

        if (network.secure && !network.saved) {
            if (root.expandedSsid !== network.ssid) {
                root._expandPassword(network);
                return;
            }

            if (root.passwordText.trim().length === 0)
                return;

            Network.connectTo(network, root.passwordText, true);
            root._expandPassword(null);
            return;
        }

        Network.connectTo(network, "", network.saved);
        root._expandPassword(null);
    }

    function _forgetNetwork(network) {
        if (!network)
            return;
        if (root.expandedSsid === network.ssid)
            root._expandPassword(null);
        Network.forgetNetwork(network);
    }

    onVisibleChanged: {
        if (visible) {
            Network.maybeRefreshScan();
        } else {
            root._expandPassword(null);
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
                    text: "Network"
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
                label: Network.scanState === "starting" ? "Starting"
                       : Network.scanState === "running" ? "Scanning"
                       : "Refresh"
                iconPath: Network.scanning ? "lucide/loader-circle.svg" : "lucide/rotate-cw.svg"
                enabled: Network.ready
                         && Network.availability === "ready"
                         && Network.scanState === "idle"
                         && !Network.scanRequestPending
                active: Network.scanState !== "idle"
                pending: Network.scanRequestPending
                spinning: Network.scanning
                onClicked: Network.scan()
            }

            HeaderChip {
                label: Network.powerPending
                       ? (Network.enabled ? "Turning off" : "Turning on")
                       : (Network.enabled ? "On" : "Off")
                enabled: Network.ready && Network.present && Network.rfkillAvailable
                active: Network.enabled || Network.powerPending
                pending: Network.powerPending
                onClicked: Network.toggleEnabled()
            }

            HeaderChip {
                label: Network.airplanePending ? "Working" : "Flight"
                enabled: Network.ready && Network.rfkillAvailable
                active: Network.airplaneMode || Network.airplanePending
                pending: Network.airplanePending
                onClicked: Network.toggleAirplaneMode()
            }
        }

        Text {
            visible: Network.lastError !== ""
            width: parent.width
            text: Network.lastError
            color: Theme.colorError
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            wrapMode: Text.WordWrap
        }

        StateCard {
            visible: root._showLoadingState
            iconPath: "lucide/loader-circle.svg"
            title: "Waiting for daemon"
            message: "Network status is not available yet."
            spinning: true
        }

        StateCard {
            visible: root._showUnavailableState
            iconPath: root._availabilityIcon()
            title: Network.availabilityTitle()
            message: Network.availabilityMessage()
        }

        StateCard {
            visible: root._showDisabledState
            iconPath: Network.airplaneMode ? "lucide/triangle-alert.svg" : Theme.iconWifiZeroStatus
            title: Network.availabilityTitle()
            message: Network.availabilityMessage()
        }

        StateCard {
            visible: root._showScanningState
            iconPath: "lucide/loader-circle.svg"
            title: "Scanning for networks"
            message: "Nearby Wi-Fi networks will appear here."
            spinning: true
        }

        StateCard {
            visible: root._showEmptyState
            iconPath: Theme.iconWifiZeroStatus
            title: "No networks found"
            message: "Use Refresh to scan nearby Wi-Fi networks."
        }

        Flickable {
            id: listArea
            visible: root._showList
            width: parent.width
            height: Math.min(listCol.implicitHeight, root._listMaxHeight)
            contentHeight: listCol.implicitHeight
            contentWidth: width
            clip: true
            interactive: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: listCol
                width: listArea.width
                spacing: Theme.spaceXs

                SectionLabel {
                    visible: Network.currentNetworks.length > 0
                    text: "Current"
                }

                Repeater {
                    model: Network.currentNetworks

                    delegate: NetworkCard {
                        required property var modelData
                        width: listCol.width
                        network: modelData
                    }
                }

                SectionLabel {
                    visible: Network.savedVisibleNetworks.length > 0
                    text: "Saved"
                }

                Repeater {
                    model: Network.savedVisibleNetworks

                    delegate: NetworkCard {
                        required property var modelData
                        width: listCol.width
                        network: modelData
                    }
                }

                SectionLabel {
                    visible: Network.availableNetworks.length > 0
                    text: "Available"
                }

                Repeater {
                    model: Network.availableNetworks

                    delegate: NetworkCard {
                        required property var modelData
                        width: listCol.width
                        network: modelData
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

    component NetworkCard: Rectangle {
        id: card

        required property var network

        readonly property bool _expanded: root.expandedSsid === network.ssid
        readonly property bool _passwordRequired: network && network.secure && !network.saved && !network.current
        readonly property bool _busy: Network.networkPending(network.ssid)
                                   || (network.current && Network.pendingDisconnect)
        readonly property bool _showForget: network && network.saved && !network.current && !card._busy

        radius: Theme.radiusSm
        color: cardHover.containsMouse ? Theme.surfaceHover : Theme.bgSurfaceRaised
        border.color: network && network.current ? Theme.borderAccent : Theme.borderSubtle
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
                    color: network && network.current ? Theme.surfaceActive : Theme.bgSurface

                    SvgIcon {
                        anchors.centerIn: parent
                        iconPath: Network.networkIconPath(network)
                        size: Theme.iconSize
                        color: network && network.current ? Theme.accentBlue : Theme.fgSecondary
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2

                    Text {
                        text: network ? network.ssid : ""
                        color: Theme.fgPrimary
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        font.weight: Theme.weightMedium
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }

                    Text {
                        text: root._networkSubtitle(network)
                        color: Theme.fgMuted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }

                SvgIcon {
                    visible: network && network.secure
                    iconPath: "lucide/lock.svg"
                    size: Theme.fontSmall + 2
                    color: Theme.fgMuted
                }

                HeaderChip {
                    label: Network.primaryActionLabel(network)
                    active: network && network.current
                    enabled: network && network.current
                             ? !Network.pendingDisconnect
                             : Network.canConnect(network)
                    pending: false
                    onClicked: root._connectNetwork(network)
                }

                HeaderChip {
                    visible: card._showForget
                    label: "Forget"
                    enabled: true
                    onClicked: root._forgetNetwork(network)
                }
            }

            ColumnLayout {
                visible: card._passwordRequired && card._expanded
                Layout.fillWidth: true
                spacing: Theme.spaceSm

                Rectangle {
                    Layout.fillWidth: true
                    radius: Theme.radiusXs
                    color: Theme.bgSurface
                    border.color: passwordField.activeFocus ? Theme.borderAccent : Theme.borderSubtle
                    border.width: 1
                    implicitHeight: passwordField.implicitHeight + Theme.spaceSm * 2

                    Item {
                        anchors.fill: parent
                        anchors.margins: Theme.spaceSm

                        TextInput {
                            id: passwordField
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.passwordText
                            color: Theme.fgPrimary
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody
                            echoMode: TextInput.Password
                            selectByMouse: true
                            onTextChanged: root.passwordText = text
                            Keys.onReturnPressed: root._connectNetwork(card.network)

                            Component.onCompleted: {
                                if (card._expanded)
                                    forceActiveFocus();
                            }
                        }

                        Text {
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            visible: root.passwordText.length === 0
                            text: "Password"
                            color: Theme.fgMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceSm

                    Item { Layout.fillWidth: true }

                    HeaderChip {
                        label: "Cancel"
                        enabled: true
                        onClicked: root._expandPassword(null)
                    }

                    HeaderChip {
                        label: "Connect"
                        enabled: root.passwordText.trim().length > 0 && Network.canConnect(card.network)
                        active: true
                        onClicked: root._connectNetwork(card.network)
                    }
                }
            }
        }
    }
}
