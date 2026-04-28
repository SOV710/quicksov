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
    property Item activePasswordField: null

    readonly property int _visibleNetworkCount: Network.visibleNetworkCount
    readonly property bool _showInlinePassword: activePasswordField && activePasswordField.visible
    readonly property bool _showLoadingState: !Network.ready
    readonly property bool _showUnavailableState: Network.ready && Network.isUnavailable
    readonly property bool _showDisabledState: Network.ready && Network.isDisabled
    readonly property bool _showEmptyState: Network.ready
                                         && Network.availability === "ready"
                                         && root._visibleNetworkCount === 0
                                         && !Network.scanning
    readonly property bool _showScanningState: Network.ready
                                             && Network.availability === "ready"
                                             && root._visibleNetworkCount === 0
                                             && Network.scanning
    readonly property bool _showList: Network.ready
                                    && Network.availability === "ready"
                                    && root._visibleNetworkCount > 0
    readonly property real _listMaxHeight: Math.max(
        Theme.spaceXxl * 3,
        Theme.networkPanelMaxHeight - headerRow.implicitHeight - Theme.spaceMd * 5
    )
    readonly property string keyboardFocusPolicy: root._showInlinePassword ? "on_demand" : "none"

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

    function _networkDetailText(network) {
        if (!network)
            return "";

        var parts = [];
        if (network.bandLabel)
            parts.push(network.bandLabel);
        if (network.signalPct >= 0)
            parts.push(String(network.signalPct) + "%");
        if (parts.length === 0 && network.current && Network.currentIpv4 !== "")
            parts.push(Network.currentIpv4);

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

    function _updateActivePasswordField(field, active) {
        if (active) {
            root.activePasswordField = field;
            return;
        }

        if (root.activePasswordField === field)
            root.activePasswordField = null;
    }

    function activateKeyboardFocus() {
        if (root._showInlinePassword && root.activePasswordField)
            root.activePasswordField.forceActiveFocus();
    }

    function handleEscape() {
        if (!root._showInlinePassword)
            return false;

        root._expandPassword(null);
        return true;
    }

    onVisibleChanged: {
        if (!visible)
            root._expandPassword(null);
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
                    enabled: Network.ready
                             && Network.availability === "ready"
                             && !Network.scanPending
                    active: Network.scanning || Network.scanPending
                    spinning: Network.scanning || Network.scanPending
                    onClicked: Network.toggleScan()
                }

                IconCircleButton {
                    iconPath: Theme.iconPowerStatus
                    enabled: Network.ready && Network.present && Network.rfkillAvailable
                    active: Network.enabled || Network.powerPending
                    spinning: Network.powerPending
                    onClicked: Network.toggleEnabled()
                }

                IconCircleButton {
                    iconPath: Theme.iconFlightStatus
                    enabled: Network.ready && Network.rfkillAvailable
                    active: Network.airplaneMode || Network.airplanePending
                    spinning: Network.airplanePending
                    onClicked: Network.toggleAirplaneMode()
                }
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
            message: "Nearby Wi-Fi networks will appear after a scan."
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
                    visible: Network.savedColumnNetworks.length > 0
                    width: parent.width
                    text: "Saved"
                }

                Repeater {
                    model: Network.savedColumnNetworks

                    delegate: NetworkCard {
                        required property var modelData
                        width: listCol.width
                        network: modelData
                    }
                }

                SectionLabel {
                    visible: Network.availableColumnNetworks.length > 0
                    width: parent.width
                    text: "Available"
                }

                Repeater {
                    model: Network.availableColumnNetworks

                    delegate: NetworkCard {
                        required property var modelData
                        width: listCol.width
                        network: modelData
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

    component NetworkCard: Rectangle {
        id: card

        required property var network

        readonly property bool _expanded: root.expandedSsid === network.ssid
        readonly property bool _passwordRequired: network && network.secure && !network.saved && !network.current
        readonly property bool _showInlinePassword: card._passwordRequired && card._expanded
        readonly property bool _busy: Network.networkPending(network.ssid)
                                   || (network.current && Network.pendingDisconnect)
        readonly property bool _showForget: network && network.saved && !network.current && !card._busy
        readonly property bool _currentActionEnabled: Network.canMutate()
                                                     && !card._busy
                                                     && !Network.pendingDisconnect
                                                     && Network.pendingConnectSsid === ""
        readonly property bool _connectActionEnabled: !card._showInlinePassword && Network.canConnect(network)
        readonly property bool _forgetActionEnabled: Network.canMutate()
                                                    && !card._busy
                                                    && !Network.pendingDisconnect
                                                    && Network.pendingConnectSsid === ""
        readonly property bool _showMetaRow: currentStateIcon.visible
                                             || savedStateIcon.visible
                                             || secureStateIcon.visible
                                             || detailText.text.length > 0
        readonly property real _actionButtonSize: Theme.iconSize + Theme.spaceLg

        radius: Theme.radiusSm
        color: network && network.current
               ? Theme.overlay(cardHover.containsMouse ? Theme.surfaceHover : Theme.bgSurfaceRaised,
                               Theme.accentBlue,
                               0.14)
               : cardHover.containsMouse
                 ? Theme.surfaceHover
                 : Theme.bgSurfaceRaised
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

                    RowLayout {
                        visible: card._showMetaRow
                        spacing: Theme.spaceXs
                        Layout.fillWidth: true

                        SvgIcon {
                            id: currentStateIcon
                            visible: network && network.current
                            iconPath: Theme.iconRadioButtonCheckedStatus
                            size: Theme.fontSmall + 2
                            color: Theme.accentBlue
                        }

                        SvgIcon {
                            id: savedStateIcon
                            visible: network && !network.current && network.saved
                            iconPath: Theme.iconBookmarkStatus
                            size: Theme.fontSmall + 2
                            color: Theme.accentBlue
                        }

                        SvgIcon {
                            id: secureStateIcon
                            visible: network && network.secure
                            iconPath: Theme.iconLockStatus
                            size: Theme.fontSmall + 2
                            color: Theme.fgMuted
                        }

                        Text {
                            id: detailText
                            visible: text.length > 0
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            text: root._networkDetailText(network)
                            color: Theme.fgMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            elide: Text.ElideRight
                        }
                    }
                }

                RowLayout {
                    spacing: Theme.spaceXs

                    IconCircleButton {
                        visible: card._busy || !card._showInlinePassword
                        buttonSize: card._actionButtonSize
                        iconPath: card._busy
                                  ? Theme.iconRefreshStatus
                                  : (network && network.current ? Theme.iconCloseStatus : Theme.iconCheckStatus)
                        enabled: card._busy
                                 ? false
                                 : (network && network.current ? card._currentActionEnabled : card._connectActionEnabled)
                        active: !card._busy
                        spinning: card._busy
                        onClicked: root._connectNetwork(network)
                    }

                    IconCircleButton {
                        visible: card._showForget
                        buttonSize: card._actionButtonSize
                        iconPath: Theme.iconDeleteStatus
                        enabled: card._forgetActionEnabled
                        danger: true
                        onClicked: root._forgetNetwork(network)
                    }
                }
            }

            ColumnLayout {
                visible: card._showInlinePassword
                Layout.fillWidth: true
                spacing: Theme.spaceSm

                onVisibleChanged: {
                    root._updateActivePasswordField(passwordField, visible);
                }

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
                            Component.onDestruction: root._updateActivePasswordField(passwordField, false)
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
                    spacing: Theme.spaceXs

                    Item { Layout.fillWidth: true }

                    IconCircleButton {
                        buttonSize: card._actionButtonSize
                        iconPath: Theme.iconCloseStatus
                        enabled: true
                        onClicked: root._expandPassword(null)
                    }

                    IconCircleButton {
                        buttonSize: card._actionButtonSize
                        iconPath: Theme.iconCheckStatus
                        enabled: root.passwordText.trim().length > 0 && Network.canConnect(card.network)
                        active: true
                        onClicked: root._connectNetwork(card.network)
                    }
                }
            }
        }
    }
}
