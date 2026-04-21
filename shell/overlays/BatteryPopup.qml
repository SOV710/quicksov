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

    width: Theme.batteryPanelWidth
    implicitHeight: height
    height: Math.min(contentCol.implicitHeight + Theme.spaceMd * 2, Theme.batteryPanelMaxHeight)
    radius: Theme.radiusXl
    color: Theme.bgSurface
    border.color: Theme.borderDefault
    border.width: 1
    opacity: visible ? Theme.opacityPopup : 0
    clip: true

    readonly property var _profiles: ["power-saver", "balanced", "performance"]
    readonly property color _heroTone: root._heroColor()

    function _heroIcon() {
        if (Battery.isUnavailable)
            return "lucide/triangle-alert.svg";
        if (!Battery.hasBattery)
            return "lucide/battery-warning.svg";
        return Theme.batteryIconForLevel(Battery.percentage, Battery.chargeStatus);
    }

    function _heroColor() {
        if (Battery.isUnavailable)
            return Theme.colorError;
        if (!Battery.hasBattery)
            return Theme.fgMuted;
        if (Battery.isCharging || Battery.isFullyCharged)
            return Theme.colorSuccess;
        if (Battery.percentage <= 15)
            return Theme.colorError;
        if (Battery.percentage <= 30)
            return Theme.colorWarning;
        return Theme.accentBlue;
    }

    function _metricValueText(value, suffix, decimals) {
        if (typeof value !== "number")
            return "Unavailable";
        var places = decimals !== undefined ? decimals : 1;
        return value.toFixed(places) + (suffix || "");
    }

    function _capacityValue() {
        if (typeof Battery.energyNowWh === "number" && typeof Battery.energyFullWh === "number")
            return Battery.energyNowWh.toFixed(1) + " / " + Battery.energyFullWh.toFixed(1) + " Wh";
        if (typeof Battery.energyFullWh === "number")
            return Battery.energyFullWh.toFixed(1) + " Wh";
        return "Unavailable";
    }

    function _capacityDetail() {
        if (typeof Battery.energyDesignWh === "number")
            return "Design " + Battery.energyDesignWh.toFixed(1) + " Wh";
        return "Current / full capacity";
    }

    function _profileAvailableMessage() {
        if (Battery.isUnavailable)
            return "Power profile control unavailable while battery backend is offline.";
        if (!Battery.powerProfileAvailable)
            return "Power profile service unavailable on this system.";
        if (Battery.profilePending)
            return "Applying " + Battery.profileLabel(Battery.pendingProfile) + "…";
        return "";
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
            width: parent.width
            spacing: Theme.spaceMd

            Rectangle {
                Layout.preferredWidth: 56
                Layout.preferredHeight: 56
                radius: Theme.radiusSm
                color: Qt.rgba(root._heroTone.r, root._heroTone.g, root._heroTone.b, 0.12)
                border.color: Qt.rgba(root._heroTone.r, root._heroTone.g, root._heroTone.b, 0.22)
                border.width: 1

                SvgIcon {
                    anchors.centerIn: parent
                    iconPath: root._heroIcon()
                    size: 28
                    color: root._heroColor()
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceSm

                    Text {
                        text: Battery.hasBattery ? Math.round(Battery.percentage) + "%" : "Battery"
                        color: Theme.fgPrimary
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontDisplay
                        font.weight: Theme.weightSemibold
                    }

                    Text {
                        text: Battery.hasBattery ? Battery.displayStatus() : (Battery.noBattery ? "No battery" : "Unavailable")
                        color: Battery.hasBattery ? root._heroColor() : Theme.fgMuted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontLabel
                        font.weight: Theme.weightMedium
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                    }
                }

                Text {
                    text: Battery.timeEstimateText()
                    color: Theme.fgSecondary
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }
            }
        }

        Text {
            visible: Battery.lastError !== ""
            width: parent.width
            text: Battery.lastError
            color: Theme.colorError
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            wrapMode: Text.WordWrap
        }

        StateCard {
            visible: Battery.noBattery
            iconPath: "lucide/battery-warning.svg"
            title: "No battery detected"
            message: "This system does not report a laptop battery, but power profile controls may still be available."
        }

        StateCard {
            visible: Battery.isUnavailable
            iconPath: "lucide/triangle-alert.svg"
            title: "Battery backend unavailable"
            message: "UPower is not reachable right now. Battery metrics and profile controls are temporarily unavailable."
        }

        Column {
            visible: Battery.hasBattery
            width: parent.width
            spacing: Theme.spaceSm

            RowLayout {
                width: parent.width
                spacing: Theme.spaceSm

                MetricCard {
                    Layout.fillWidth: true
                    label: "Power Source"
                    value: Battery.sourceLabel()
                    detail: Battery.displayStatus()
                }

                MetricCard {
                    Layout.fillWidth: true
                    label: "Battery Health"
                    value: root._metricValueText(Battery.healthPercent, "%", 0)
                    detail: typeof Battery.energyDesignWh === "number"
                            ? "Derived from full vs design"
                            : "Design capacity unavailable"
                }
            }

            RowLayout {
                width: parent.width
                spacing: Theme.spaceSm

                MetricCard {
                    Layout.fillWidth: true
                    label: "Charge Rate"
                    value: root._metricValueText(Battery.energyRateW, " W", 1)
                    detail: Battery.onBattery ? "Current discharge rate" : "Current charge rate"
                }

                MetricCard {
                    Layout.fillWidth: true
                    label: "Capacity"
                    value: root._capacityValue()
                    detail: root._capacityDetail()
                }
            }
        }

        Rectangle {
            width: parent.width
            radius: Theme.radiusSm
            color: Theme.bgSurfaceRaised
            border.color: Theme.borderSubtle
            border.width: 1
            implicitHeight: profileCol.implicitHeight + Theme.spaceSm * 2

            Column {
                id: profileCol
                anchors.fill: parent
                anchors.margins: Theme.spaceSm
                spacing: Theme.spaceSm

                RowLayout {
                    width: parent.width

                    Text {
                        text: "Power Profile"
                        color: Theme.fgPrimary
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        font.weight: Theme.weightMedium
                        Layout.fillWidth: true
                    }

                    Text {
                        text: Battery.profileLabel(Battery.powerProfile)
                        color: Theme.fgMuted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        font.weight: Theme.weightMedium
                        visible: Battery.powerProfile !== ""
                    }
                }

                Rectangle {
                    width: parent.width
                    height: Theme.barHeight + Theme.spaceXs
                    radius: Theme.radiusSm
                    color: Theme.bgSurface
                    border.color: Theme.borderSubtle
                    border.width: 1
                    opacity: Battery.powerProfileAvailable ? 1.0 : 0.55

                    RowLayout {
                        anchors.fill: parent
                        anchors.margins: 3
                        spacing: 3

                        Repeater {
                            model: root._profiles

                            delegate: Rectangle {
                                required property string modelData

                                readonly property bool _current: Battery.powerProfile === modelData
                                readonly property bool _pending: Battery.pendingProfile === modelData
                                readonly property bool _enabled: Battery.canSetPowerProfile(modelData)

                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                radius: Theme.radiusXs
                                color: _current || _pending
                                       ? Theme.surfaceActive
                                       : profileMouse.containsMouse
                                         ? Theme.surfaceHover
                                         : "transparent"
                                border.color: (_current || _pending) ? Theme.borderAccent : "transparent"
                                border.width: (_current || _pending) ? 1 : 0
                                scale: profileMouse.pressed ? 0.98 : 1.0

                                Behavior on color { ColorAnimation { duration: Theme.motionFast } }
                                Behavior on scale { NumberAnimation { duration: Theme.motionFast } }

                                Text {
                                    anchors.centerIn: parent
                                    text: Battery.profileLabel(modelData)
                                    color: (_current || _pending) ? Theme.accentBlue : Theme.fgSecondary
                                    opacity: Battery.powerProfileAvailable ? 1.0 : 0.8
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontBody
                                    font.weight: (_current || _pending) ? Theme.weightMedium : Theme.weightRegular
                                }

                                MouseArea {
                                    id: profileMouse
                                    anchors.fill: parent
                                    enabled: parent._enabled
                                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                                    onClicked: Battery.setPowerProfile(modelData)
                                }
                            }
                        }
                    }
                }

                Text {
                    visible: text !== ""
                    width: parent.width
                    text: root._profileAvailableMessage()
                    color: Battery.powerProfileAvailable ? Theme.fgMuted : Theme.colorWarning
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    wrapMode: Text.WordWrap
                }
            }
        }
    }

    component MetricCard: Rectangle {
        id: metricCard

        property string label: ""
        property string value: ""
        property string detail: ""

        implicitHeight: metricCol.implicitHeight + Theme.spaceSm * 2
        radius: Theme.radiusSm
        color: Theme.bgSurfaceRaised
        border.color: Theme.borderSubtle
        border.width: 1

        Column {
            id: metricCol
            anchors.fill: parent
            anchors.margins: Theme.spaceSm
            spacing: 4

            Text {
                text: metricCard.label
                color: Theme.fgMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                font.weight: Theme.weightMedium
            }

            Text {
                text: metricCard.value
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Theme.weightSemibold
                wrapMode: Text.WordWrap
            }

            Text {
                visible: metricCard.detail !== ""
                text: metricCard.detail
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap
            }
        }
    }

    component StateCard: Rectangle {
        id: stateCard

        property string iconPath: ""
        property string title: ""
        property string message: ""

        width: parent.width
        radius: Theme.radiusSm
        color: Theme.bgSurfaceRaised
        border.color: Theme.borderSubtle
        border.width: 1
        implicitHeight: stateCol.implicitHeight + Theme.spaceMd * 2

        Column {
            id: stateCol
            anchors.fill: parent
            anchors.margins: Theme.spaceMd
            spacing: Theme.spaceXs

            SvgIcon {
                iconPath: stateCard.iconPath
                size: Theme.iconSize + 2
                color: Theme.colorWarning
            }

            Text {
                text: stateCard.title
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Theme.weightSemibold
            }

            Text {
                text: stateCard.message
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                wrapMode: Text.WordWrap
                width: parent.width
            }
        }
    }
}
