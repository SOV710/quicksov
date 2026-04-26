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
    implicitHeight: contentCol.implicitHeight + Theme.spaceMd * 2

    readonly property var _profiles: Array.isArray(Battery.powerProfileChoices)
                                     && Battery.powerProfileChoices.length >= 2
                                     ? Battery.powerProfileChoices
                                     : ["power-saver", "balanced", "performance"]
    readonly property var _batteryPalette: Theme.batteryPalette(Battery.chargeStatus, Battery.availability)
    readonly property real _healthProgress: typeof Battery.healthPercent === "number"
                                            ? Math.max(0, Math.min(1, Battery.healthPercent / 100.0))
                                            : -1
    readonly property real _energyProgress: (typeof Battery.energyNowWh === "number"
                                             && typeof Battery.energyFullWh === "number"
                                             && Battery.energyFullWh > 0)
                                            ? Math.max(0, Math.min(1, Battery.energyNowWh / Battery.energyFullWh))
                                            : -1
    readonly property bool _powerControlEnabled: Battery.powerProfileAvailable
                                                 && !Battery.profilePending
    readonly property bool _powerControlUnavailable: !Battery.powerProfileAvailable
    readonly property bool _showDegradedWarning: Battery.powerProfile === "performance"
                                                 && typeof Battery.powerProfileDegradedReason === "string"
                                                 && Battery.powerProfileDegradedReason !== ""
                                                 && Battery.powerProfileAvailable
                                                 && !Battery.profilePending
    readonly property int _profileVisualIndex: root._resolveProfileVisualIndex()

    property real _heroPhase: 0.0
    property bool _profileDragging: false
    property int _profileDragIndex: root._defaultProfileIndex()

    function _defaultProfileIndex() {
        var balanced = root._profiles.indexOf("balanced");
        return balanced >= 0 ? balanced : 0;
    }

    function _profileIndex(profile) {
        var idx = root._profiles.indexOf(profile);
        return idx >= 0 ? idx : root._defaultProfileIndex();
    }

    function _profileAt(index) {
        var clamped = Math.max(0, Math.min(root._profiles.length - 1, index));
        return root._profiles[clamped];
    }

    function _nearestProfileIndex(x, width) {
        if (width <= 0)
            return root._defaultProfileIndex();
        return Math.max(0, Math.min(
            root._profiles.length - 1,
            Math.floor((Math.max(0, Math.min(width - 1, x)) / width) * root._profiles.length)
        ));
    }

    function _resolveProfileVisualIndex() {
        if (root._profileDragging)
            return root._profileDragIndex;
        if (Battery.profilePending)
            return root._profileIndex(Battery.pendingProfile);
        return root._profileIndex(Battery.powerProfile);
    }

    function _profileStatusText() {
        if (Battery.profilePending)
            return Battery.profileLabel(Battery.pendingProfile);
        if (Battery.powerProfile !== "")
            return Battery.profileLabel(Battery.powerProfile);
        return "";
    }

    function _profileAvailableMessage() {
        if (Battery.profilePending)
            return "Applying " + Battery.profileLabel(Battery.pendingProfile) + "…";
        if (!Battery.powerProfileAvailable) {
            switch (Battery.powerProfileReason) {
            case "unsupported":
                return "This system does not expose at least two standard power modes.";
            case "service_unavailable":
                return "power-profiles-daemon is not available right now. Power controls are disabled.";
            case "permission_denied":
                return "power-profiles-daemon denied this power-mode change request.";
            case "write_failed":
                return "The requested power mode could not be applied.";
            default:
                return "Power mode control unavailable on this system.";
            }
        }
        if (root._showDegradedWarning) {
            switch (Battery.powerProfileDegradedReason) {
            case "lap-detected":
                return "Performance mode is limited because lap detection is active.";
            case "high-operating-temperature":
                return "Performance mode is limited because the system is running hot.";
            default:
                return "Performance mode is temporarily limited by the system.";
            }
        }
        return "";
    }

    NumberAnimation on _heroPhase {
        from: 0
        to: 1
        duration: Theme.batteryHeroCycleDuration
        loops: Animation.Infinite
        running: Battery.hasBattery
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

            Repeater {
                model: Battery.presentBatteries

                delegate: Column {
                    id: heroDelegate

                    required property var modelData

                    width: parent.width
                    spacing: Theme.spaceSm

                    readonly property var _heroPalette: Theme.batteryPalette(modelData.state || "unknown", Battery.availability)
                    property real _heroDisplayedLevel: Math.max(0, Math.min(1, Number(modelData.level || 0) / 100.0))

                    Behavior on _heroDisplayedLevel {
                        NumberAnimation {
                            duration: Theme.batteryHeroSettleDuration
                            easing.type: Easing.OutCubic
                        }
                    }

                    Rectangle {
                        width: parent.width
                        height: Theme.batteryHeroCardHeight
                        radius: Theme.radiusMd
                        color: Theme.chromeSubtleFillMuted
                        border.color: parent._heroPalette.frame
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
                                property real level: heroDelegate._heroDisplayedLevel
                                property real phase: root._heroPhase
                                property real frontSoftness: Theme.batteryHeroFrontSoftness
                                property real waveAmplitude: Theme.batteryHeroWaveAmplitude
                                property vector4d fillColor: Qt.vector4d(
                                    heroDelegate._heroPalette.fill.r,
                                    heroDelegate._heroPalette.fill.g,
                                    heroDelegate._heroPalette.fill.b,
                                    heroDelegate._heroPalette.fill.a
                                )
                                property vector4d deepColor: Qt.vector4d(
                                    heroDelegate._heroPalette.deep.r,
                                    heroDelegate._heroPalette.deep.g,
                                    heroDelegate._heroPalette.deep.b,
                                    heroDelegate._heroPalette.deep.a
                                )
                                property vector4d backgroundColor: Qt.vector4d(
                                    heroDelegate._heroPalette.background.r,
                                    heroDelegate._heroPalette.background.g,
                                    heroDelegate._heroPalette.background.b,
                                    heroDelegate._heroPalette.background.a
                                )
                                property vector4d highlightColor: Qt.vector4d(
                                    heroDelegate._heroPalette.highlight.r,
                                    heroDelegate._heroPalette.highlight.g,
                                    heroDelegate._heroPalette.highlight.b,
                                    heroDelegate._heroPalette.highlight.a
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
                            text: Math.round(Number(heroDelegate.modelData.level || 0)) + "%"
                            color: heroDelegate._heroPalette.accent
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontHero
                            font.weight: Theme.weightSemibold
                            font.features: { "tnum": 1 }
                        }

                        Text {
                            Layout.alignment: Qt.AlignBottom
                            text: String(heroDelegate.modelData.name || "")
                            color: Theme.fgMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody
                            font.weight: Theme.weightMedium
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
                                visible: Theme.batteryChargeBadgeIcon(heroDelegate.modelData.state || "") !== ""
                                width: Theme.batteryHeroChargeBadgeSize
                                height: Theme.batteryHeroChargeBadgeSize
                                radius: width / 2
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                color: Theme.overlay(Theme.bgSurfaceRaised, heroDelegate._heroPalette.accent, 0.20)
                                border.color: Theme.withAlpha(heroDelegate._heroPalette.accent, 0.26)
                                border.width: 1

                                SvgIcon {
                                    anchors.centerIn: parent
                                    iconPath: Theme.batteryChargeBadgeIcon(heroDelegate.modelData.state || "")
                                    size: Theme.batteryHeroChargeBadgeSize - Theme.spaceXs
                                    color: heroDelegate._heroPalette.accent
                                }
                            }
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
            message: "UPower is not readable right now. Battery metrics are temporarily unavailable."
        }

        Rectangle {
            visible: Battery.hasBattery
            width: parent.width
            radius: Theme.radiusSm
            color: Theme.statusDockFill
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
                    progress: root._energyProgress
                    valueText: typeof Battery.energyNowWh === "number"
                               ? Battery.energyNowWh.toFixed(1)
                               : "\u2014"
                    unitText: typeof Battery.energyNowWh === "number" ? "Wh" : ""
                    label: "Energy"
                    unavailable: root._energyProgress < 0
                }
            }
        }

        Rectangle {
            width: parent.width
            radius: Theme.radiusSm
            color: Theme.statusDockFill
            implicitHeight: controlsCol.childrenRect.height + Theme.batteryControlCardPadding * 2

            Column {
                id: controlsCol
                x: Theme.batteryControlCardPadding
                y: Theme.batteryControlCardPadding
                width: parent.width - Theme.batteryControlCardPadding * 2
                spacing: Theme.spaceSm

                RowLayout {
                    width: parent.width

                    Text {
                        text: "Power Mode"
                        color: Theme.fgPrimary
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        font.weight: Theme.weightMedium
                        Layout.fillWidth: true
                    }

                    Text {
                        visible: text !== ""
                        text: root._profileStatusText()
                        color: root._powerControlUnavailable ? Theme.fgDisabled : Theme.fgMuted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        font.weight: Theme.weightMedium
                    }
                }

                Rectangle {
                    id: powerModeTrack
                    width: parent.width
                    height: Theme.batteryControlTrackHeight
                    radius: Theme.radiusSm
                    color: Theme.chromeSubtleFillMuted
                    border.color: Theme.borderSubtle
                    border.width: 1
                    opacity: root._powerControlUnavailable ? 0.42 : Battery.profilePending ? 0.84 : 1.0

                    Item {
                        id: trackInner
                        anchors.fill: parent
                        anchors.margins: Theme.batteryControlThumbInset

                        readonly property real segmentWidth: width / root._profiles.length

                        Rectangle {
                            id: sliderThumb
                            x: trackInner.segmentWidth * root._profileVisualIndex
                            y: 0
                            width: trackInner.segmentWidth
                            height: trackInner.height
                            radius: Theme.radiusXs
                            color: root._powerControlUnavailable
                                   ? Theme.withAlpha(Theme.fgMuted, 0.10)
                                   : Theme.overlay(Theme.bgSurfaceRaised, root._batteryPalette.accent, 0.18)
                            border.color: root._powerControlUnavailable
                                          ? Theme.withAlpha(Theme.fgMuted, 0.10)
                                          : Theme.withAlpha(root._batteryPalette.accent, 0.26)
                            border.width: 1

                            Behavior on x {
                                NumberAnimation {
                                    duration: Theme.motionNormal
                                    easing.type: Easing.OutCubic
                                }
                            }
                        }

                        Repeater {
                            model: root._profiles

                            delegate: Item {
                                required property string modelData
                                required property int index

                                x: trackInner.segmentWidth * index
                                width: trackInner.segmentWidth
                                height: trackInner.height

                                readonly property bool _active: index === root._profileVisualIndex

                                Column {
                                    anchors.centerIn: parent
                                    spacing: Theme.spaceXs

                                    SvgIcon {
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        iconPath: Theme.batteryPowerProfileIcon(modelData)
                                        size: Theme.batteryControlIconSize
                                        color: root._powerControlUnavailable
                                               ? Theme.fgDisabled
                                               : parent.parent._active
                                                 ? root._batteryPalette.accent
                                                 : Theme.fgSecondary
                                    }

                                    Text {
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        text: Battery.profileLabel(modelData)
                                        color: root._powerControlUnavailable
                                               ? Theme.fgDisabled
                                               : parent.parent._active
                                                 ? Theme.fgPrimary
                                                 : Theme.fgSecondary
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontSmall
                                        font.weight: parent.parent._active ? Theme.weightMedium : Theme.weightRegular
                                    }
                                }
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            enabled: root._powerControlEnabled
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor

                            onPressed: function(mouse) {
                                root._profileDragging = true;
                                root._profileDragIndex = root._nearestProfileIndex(mouse.x, trackInner.width);
                            }

                            onPositionChanged: function(mouse) {
                                if (pressed)
                                    root._profileDragIndex = root._nearestProfileIndex(mouse.x, trackInner.width);
                            }

                            onReleased: function(mouse) {
                                var targetIndex = root._nearestProfileIndex(mouse.x, trackInner.width);
                                var targetProfile = root._profileAt(targetIndex);
                                root._profileDragging = false;
                                root._profileDragIndex = targetIndex;
                                if (Battery.canSetPowerProfile(targetProfile))
                                    Battery.setPowerProfile(targetProfile);
                            }

                            onCanceled: {
                                root._profileDragging = false;
                                root._profileDragIndex = root._profileIndex(Battery.powerProfile);
                            }
                        }
                    }
                }

                Item {
                    width: parent.width
                    implicitHeight: profileStatusText.visible ? profileStatusText.implicitHeight : 0

                    Text {
                        id: profileStatusText
                        visible: text !== ""
                        anchors.left: parent.left
                        anchors.right: parent.right
                        text: root._profileAvailableMessage()
                        color: (root._powerControlUnavailable || root._showDegradedWarning)
                               ? Theme.colorWarning
                               : Theme.fgMuted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        wrapMode: Text.WordWrap
                    }
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
