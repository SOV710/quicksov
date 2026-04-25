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

    width: parent ? parent.width : Theme.batteryPanelWidth
    implicitHeight: Math.min(contentCol.implicitHeight + Theme.spaceMd * 2, Theme.batteryPanelMaxHeight)

    readonly property var _profiles: ["power-saver", "balanced", "performance"]
    readonly property var _batteryPalette: Theme.batteryPalette(Battery.chargeStatus, Battery.availability)
    readonly property real _normalizedLevel: Battery.hasBattery ? Math.max(0, Math.min(1, Battery.percentage / 100.0)) : 0.0

    property real _heroPhase: 0.0
    property real _heroDisplayedLevel: 0.0

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

    Component.onCompleted: {
        root._heroDisplayedLevel = root._normalizedLevel;
    }

    Connections {
        target: Battery

        function onPercentageChanged() {
            root._heroDisplayedLevel = root._normalizedLevel;
        }

        function onAvailabilityChanged() {
            root._heroDisplayedLevel = root._normalizedLevel;
        }

        function onPresentChanged() {
            root._heroDisplayedLevel = root._normalizedLevel;
        }
    }

    NumberAnimation on _heroPhase {
        from: 0
        to: 1
        duration: Theme.batteryHeroCycleDuration
        loops: Animation.Infinite
        running: Battery.hasBattery
    }

    Behavior on _heroDisplayedLevel {
        enabled: Battery.hasBattery

        NumberAnimation {
            duration: Theme.batteryHeroSettleDuration
            easing.type: Easing.OutCubic
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

        Column {
            visible: Battery.hasBattery
            width: parent.width
            spacing: Theme.spaceSm

            Rectangle {
                width: parent.width
                height: Theme.batteryHeroCardHeight
                radius: Theme.radiusMd
                color: Theme.chromeSubtleFillMuted
                border.color: root._batteryPalette.frame
                border.width: 1
                antialiasing: true

                Item {
                    anchors.fill: parent
                    anchors.margins: Theme.batteryHeroInset

                    ShaderEffect {
                        anchors.fill: parent
                        blending: true

                        property real itemWidth: width
                        property real itemHeight: height
                        property real level: root._heroDisplayedLevel
                        property real phase: root._heroPhase
                        property real frontSoftness: Theme.batteryHeroFrontSoftness
                        property real waveAmplitude: Theme.batteryHeroWaveAmplitude
                        property vector4d fillColor: Qt.vector4d(
                            root._batteryPalette.fill.r,
                            root._batteryPalette.fill.g,
                            root._batteryPalette.fill.b,
                            root._batteryPalette.fill.a
                        )
                        property vector4d deepColor: Qt.vector4d(
                            root._batteryPalette.deep.r,
                            root._batteryPalette.deep.g,
                            root._batteryPalette.deep.b,
                            root._batteryPalette.deep.a
                        )
                        property vector4d backgroundColor: Qt.vector4d(
                            root._batteryPalette.background.r,
                            root._batteryPalette.background.g,
                            root._batteryPalette.background.b,
                            root._batteryPalette.background.a
                        )
                        property vector4d highlightColor: Qt.vector4d(
                            root._batteryPalette.highlight.r,
                            root._batteryPalette.highlight.g,
                            root._batteryPalette.highlight.b,
                            root._batteryPalette.highlight.a
                        )
                        property real innerRadius: Theme.batteryHeroInnerRadius

                        fragmentShader: Qt.resolvedUrl("../shaders/qsb/battery_liquid_field.frag.qsb")
                    }
                }
            }

            RowLayout {
                width: parent.width
                spacing: Theme.spaceSm

                Text {
                    text: Math.round(Battery.percentage) + "%"
                    color: root._batteryPalette.accent
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontHero
                    font.weight: Theme.weightSemibold
                    font.features: { "tnum": 1 }
                }

                Item {
                    Layout.fillWidth: true
                }

                Item {
                    Layout.preferredWidth: Theme.batteryHeroSourceIconSize + Theme.batteryHeroChargeBadgeSize / 2
                    Layout.preferredHeight: Theme.batteryHeroSourceIconSize + Theme.batteryHeroChargeBadgeSize / 2

                    SvgIcon {
                        anchors.centerIn: parent
                        iconPath: Theme.batterySourceIcon(Battery.onBattery)
                        size: Theme.batteryHeroSourceIconSize
                        color: Theme.fgPrimary
                    }

                    Rectangle {
                        visible: Theme.batteryChargeBadgeIcon(Battery.chargeStatus) !== ""
                        width: Theme.batteryHeroChargeBadgeSize
                        height: Theme.batteryHeroChargeBadgeSize
                        radius: width / 2
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        color: Theme.overlay(Theme.bgSurfaceRaised, root._batteryPalette.accent, 0.20)
                        border.color: Theme.withAlpha(root._batteryPalette.accent, 0.26)
                        border.width: 1

                        SvgIcon {
                            id: badgeIcon
                            anchors.centerIn: parent
                            iconPath: Theme.batteryChargeBadgeIcon(Battery.chargeStatus)
                            size: Theme.batteryHeroChargeBadgeSize - Theme.spaceXs
                            color: root._batteryPalette.accent
                        }
                    }
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
            iconPath: Theme.batteryStateIcon(Battery.availability)
            iconColor: Theme.batteryStateColor(Battery.availability)
            title: "No battery detected"
            message: "This system does not report a laptop battery, but power profile controls may still be available."
        }

        StateCard {
            visible: Battery.isUnavailable
            iconPath: Theme.batteryStateIcon(Battery.availability)
            iconColor: Theme.batteryStateColor(Battery.availability)
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
                    height: Theme.batteryProfileSegmentHeight
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

                                RowLayout {
                                    anchors.centerIn: parent
                                    spacing: Theme.spaceXs

                                    SvgIcon {
                                        iconPath: Theme.batteryPowerProfileIcon(modelData)
                                        size: Theme.batteryProfileIconSize
                                        color: (_current || _pending) ? Theme.accentBlue : Theme.fgSecondary
                                    }

                                    Text {
                                        text: Battery.profileLabel(modelData)
                                        color: (_current || _pending) ? Theme.accentBlue : Theme.fgSecondary
                                        opacity: Battery.powerProfileAvailable ? 1.0 : 0.8
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontBody
                                        font.weight: (_current || _pending) ? Theme.weightMedium : Theme.weightRegular
                                    }
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
        property color iconColor: Theme.colorWarning

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
                color: stateCard.iconColor
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
