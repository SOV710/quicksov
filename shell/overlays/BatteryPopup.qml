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

    readonly property var _batteryPalette: Theme.batteryPalette(Battery.chargeStatus, Battery.availability)
    readonly property real _normalizedLevel: Battery.hasBattery ? Math.max(0, Math.min(1, Battery.percentage / 100.0)) : 0.0
    readonly property real _healthProgress: typeof Battery.healthPercent === "number"
                                            ? Math.max(0, Math.min(1, Battery.healthPercent / 100.0))
                                            : -1
    readonly property real _capacityProgress: (typeof Battery.energyNowWh === "number"
                                               && typeof Battery.energyFullWh === "number"
                                               && Battery.energyFullWh > 0)
                                              ? Math.max(0, Math.min(1, Battery.energyNowWh / Battery.energyFullWh))
                                              : -1

    property real _heroPhase: 0.0
    property real _heroDisplayedLevel: 0.0

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
            message: "This system does not report a laptop battery."
        }

        StateCard {
            visible: Battery.isUnavailable
            iconPath: Theme.batteryStateIcon(Battery.availability)
            iconColor: Theme.batteryStateColor(Battery.availability)
            title: "Battery backend unavailable"
            message: "UPower is not reachable right now. Battery metrics are temporarily unavailable."
        }

        Rectangle {
            visible: Battery.hasBattery
            width: parent.width
            radius: Theme.radiusSm
            color: Theme.bgSurfaceRaised
            border.color: Theme.borderSubtle
            border.width: 1
            implicitHeight: gaugesRow.implicitHeight + Theme.batteryGaugeCardPadding * 2

            RowLayout {
                id: gaugesRow
                anchors.fill: parent
                anchors.margins: Theme.batteryGaugeCardPadding
                spacing: Theme.spaceMd

                GaugeCard {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    gaugeColor: root._batteryPalette.accent
                    progress: root._healthProgress
                    valueText: typeof Battery.healthPercent === "number"
                               ? Battery.healthPercent.toFixed(0)
                               : "\u2014"
                    unitText: typeof Battery.healthPercent === "number" ? "%" : ""
                    label: "Battery Health"
                    unavailable: root._healthProgress < 0
                }

                GaugeCard {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    gaugeColor: root._batteryPalette.accent
                    progress: root._capacityProgress
                    valueText: typeof Battery.energyNowWh === "number"
                               ? Battery.energyNowWh.toFixed(1)
                               : "\u2014"
                    unitText: typeof Battery.energyNowWh === "number" ? "Wh" : ""
                    label: "Capacity"
                    unavailable: root._capacityProgress < 0
                }
            }
        }
    }

    component GaugeCard: Item {
        id: gaugeCard

        property real progress: -1
        property color gaugeColor: Theme.accentBlue
        property string valueText: ""
        property string unitText: ""
        property string label: ""
        property bool unavailable: false

        implicitWidth: Theme.batteryGaugeSize
        implicitHeight: Theme.batteryGaugeSize + Theme.spaceLg + Theme.fontBody + Theme.fontSmall

        readonly property color _trackColor: Theme.withAlpha(Theme.fgMuted, 0.16)
        readonly property color _innerFillColor: Theme.overlay(Theme.bgSurface, gaugeCard.gaugeColor, 0.06)
        readonly property color _ringColor: gaugeCard.unavailable ? Theme.fgMuted : gaugeCard.gaugeColor
        readonly property color _valueColor: gaugeCard.unavailable ? Theme.fgMuted : Theme.fgPrimary
        readonly property real _clampedProgress: Math.max(0, Math.min(1, gaugeCard.progress))

        onProgressChanged: ringCanvas.requestPaint()
        onGaugeColorChanged: ringCanvas.requestPaint()
        onUnavailableChanged: ringCanvas.requestPaint()
        onWidthChanged: ringCanvas.requestPaint()
        onHeightChanged: ringCanvas.requestPaint()

        Item {
            id: gaugeBody
            anchors.top: parent.top
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(parent.width, Theme.batteryGaugeSize)
            height: Theme.batteryGaugeSize

            Canvas {
                id: ringCanvas
                anchors.fill: parent
                antialiasing: true

                onPaint: {
                    var ctx = getContext("2d");
                    ctx.reset();

                    var stroke = Theme.batteryGaugeStrokeWidth;
                    var radius = Math.max(0, (Math.min(width, height) - stroke) / 2);
                    var centerX = width / 2;
                    var centerY = height / 2;
                    var start = Theme.batteryGaugeStartAngleDeg * Math.PI / 180.0;
                    var sweep = Theme.batteryGaugeSweepAngleDeg * Math.PI / 180.0;
                    var end = start + sweep;
                    var progressEnd = start + sweep * gaugeCard._clampedProgress;

                    ctx.lineCap = "round";

                    ctx.beginPath();
                    ctx.arc(centerX, centerY, radius, start, end, false);
                    ctx.lineWidth = stroke;
                    ctx.strokeStyle = gaugeCard._trackColor;
                    ctx.stroke();

                    if (!gaugeCard.unavailable && gaugeCard._clampedProgress > 0) {
                        var gradient = ctx.createLinearGradient(0, 0, width, height);
                        gradient.addColorStop(0.0, Theme.withAlpha(gaugeCard.gaugeColor, 0.52));
                        gradient.addColorStop(0.55, Theme.withAlpha(gaugeCard.gaugeColor, 0.84));
                        gradient.addColorStop(1.0, Theme.withAlpha(gaugeCard.gaugeColor, 1.0));

                        ctx.beginPath();
                        ctx.arc(centerX, centerY, radius, start, progressEnd, false);
                        ctx.lineWidth = stroke;
                        ctx.strokeStyle = gradient;
                        ctx.stroke();
                    }
                }
            }

            Rectangle {
                width: parent.width - Theme.batteryGaugeStrokeWidth * 2 - Theme.spaceSm
                height: width
                radius: width / 2
                anchors.centerIn: parent
                color: gaugeCard._innerFillColor
                border.color: Theme.withAlpha(gaugeCard._ringColor, gaugeCard.unavailable ? 0.08 : 0.16)
                border.width: 1
            }

            Column {
                anchors.centerIn: parent
                spacing: Theme.spaceXs

                RowLayout {
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: Theme.spaceXs

                    Text {
                        text: gaugeCard.valueText
                        color: gaugeCard._valueColor
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontDisplay
                        font.weight: Theme.weightSemibold
                        font.features: { "tnum": 1 }
                    }

                    Text {
                        visible: gaugeCard.unitText !== ""
                        Layout.alignment: Qt.AlignVCenter
                        text: gaugeCard.unitText
                        color: Theme.fgSecondary
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        font.weight: Theme.weightMedium
                    }
                }
            }
        }

        Text {
            anchors.top: gaugeBody.bottom
            anchors.topMargin: Theme.spaceSm
            anchors.horizontalCenter: parent.horizontalCenter
            text: gaugeCard.label
            color: Theme.fgMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Theme.weightMedium
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
