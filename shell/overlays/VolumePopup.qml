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

    width: Theme.volumePanelWidth
    implicitHeight: height
    height: Math.min(
        contentCol.implicitHeight + Theme.spaceMd * 2,
        Theme.volumePanelMaxHeight
    )
    radius: Theme.radiusXl
    color: Theme.bgSurface
    border.color: Theme.borderDefault
    border.width: 1
    opacity: visible ? Theme.opacityPopup : 0
    clip: true

    readonly property bool _hasAudio: Audio.ready && Audio.defaultSink !== null
    readonly property bool _hasMultipleSinks: Audio.sinks.length > 1

    function _iconPath(muted, value) {
        return Theme.iconVolumeStatus;
    }

    function _percentText(value) {
        return Math.round(value * 100) + "%";
    }

    function _accentFor(value) {
        return value > 1.0 ? Theme.accentYellow : Theme.accentBlue;
    }

    function _sinkLabel(sink) {
        if (!sink) return "No output sink";
        return sink.description || sink.name || "Unknown output";
    }

    function _streamSubtitle(stream) {
        if (!stream || !stream.title) return "";
        if (stream.title === stream.app_name) return "";
        return stream.title;
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

            Text {
                text: "Audio"
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontLabel
                font.weight: Theme.weightSemibold
                Layout.fillWidth: true
            }

            Text {
                text: Audio.streamsModel.count > 0 ? String(Audio.streamsModel.count) : ""
                visible: text !== ""
                color: Theme.fgMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.features: { "tnum": 1 }
            }
        }

        Rectangle {
            width: parent.width
            radius: Theme.radiusSm
            color: Theme.bgSurfaceRaised
            border.color: Theme.borderSubtle
            border.width: 1
            implicitHeight: masterCol.implicitHeight + Theme.spaceSm * 2

            Column {
                id: masterCol
                anchors.fill: parent
                anchors.margins: Theme.spaceSm
                spacing: Theme.spaceSm

                RowLayout {
                    width: parent.width

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        Text {
                            text: root._sinkLabel(Audio.defaultSink)
                            color: Theme.fgPrimary
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody
                            font.weight: Theme.weightMedium
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }

                        Text {
                            text: root._hasAudio ? "Master output" : "Audio unavailable"
                            color: Theme.fgMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }

                    Text {
                        id: masterPercent
                        text: root._hasAudio ? root._percentText(Audio.volume) : "—"
                        color: Audio.muted ? Theme.fgMuted : root._accentFor(Audio.volume)
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        font.weight: Theme.weightMedium
                        font.features: { "tnum": 1 }

                        MouseArea {
                            anchors.fill: parent
                            enabled: Audio.defaultSink !== null
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: {
                                if (Audio.defaultSink)
                                    Audio.setVolume(Audio.defaultSink.id, 1.0);
                            }
                        }
                    }

                    Rectangle {
                        visible: Audio.defaultSink !== null
                        width: Theme.iconSize + Theme.spaceSm
                        height: Theme.iconSize + Theme.spaceSm
                        radius: Theme.radiusXs
                        color: muteHover.containsMouse ? Theme.surfaceHover : "transparent"

                        SvgIcon {
                            anchors.centerIn: parent
                            iconPath: root._iconPath(Audio.muted, Audio.volume)
                            size: Theme.iconSize
                            color: Audio.muted ? Theme.fgMuted : root._accentFor(Audio.volume)
                        }

                        HoverHandler { id: muteHover }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (Audio.defaultSink)
                                    Audio.setMuted(Audio.defaultSink.id, !Audio.defaultSink.muted);
                            }
                        }
                    }
                }

                AudioSlider {
                    visible: Audio.defaultSink !== null
                    width: parent.width
                    modelValue: Audio.defaultSink ? (Audio.defaultSink.volume_pct / 100.0) : 0
                    accentColor: root._accentFor(liveValue)
                    muted: Audio.muted
                    onAdjusted: function(value) {
                        if (Audio.defaultSink)
                            Audio.setVolume(Audio.defaultSink.id, value);
                    }
                }
            }
        }

        Column {
            visible: root._hasMultipleSinks
            width: parent.width
            spacing: Theme.spaceSm

            Text {
                text: "Outputs"
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Theme.weightMedium
            }

            Column {
                width: parent.width
                spacing: Theme.spaceXs

                Repeater {
                    model: Audio.sinks

                    delegate: Rectangle {
                        required property var modelData

                        readonly property bool _isCurrent: Audio.defaultSink && Audio.defaultSink.name === modelData.name

                        width: parent.width
                        radius: Theme.radiusSm
                        color: _isCurrent ? Theme.surfaceActive : Theme.bgSurfaceRaised
                        border.color: _isCurrent ? Theme.borderAccent : Theme.borderSubtle
                        border.width: 1
                        implicitHeight: sinkRow.implicitHeight + Theme.spaceSm * 2

                        RowLayout {
                            id: sinkRow
                            anchors.fill: parent
                            anchors.margins: Theme.spaceSm

                            Text {
                                text: modelData.description || modelData.name || "Unknown output"
                                color: Theme.fgPrimary
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.weight: _isCurrent ? Theme.weightMedium : Theme.weightRegular
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }

                            Text {
                                text: _isCurrent ? "Default" : "Use"
                                color: _isCurrent ? Theme.accentBlue : Theme.fgMuted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                                font.weight: _isCurrent ? Theme.weightMedium : Theme.weightRegular
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            enabled: !parent._isCurrent
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: Audio.setDefaultSink(modelData.id)
                        }
                    }
                }
            }
        }

        Column {
            width: parent.width
            spacing: Theme.spaceSm

            Text {
                text: "Applications"
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Theme.weightMedium
            }

            Item {
                visible: Audio.streamsModel.count === 0
                width: parent.width
                implicitHeight: emptyText.implicitHeight + Theme.spaceSm

                Text {
                    id: emptyText
                    anchors.verticalCenter: parent.verticalCenter
                    text: "No active playback"
                    color: Theme.fgMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                }
            }

            ListView {
                id: streamList
                visible: Audio.streamsModel.count > 0
                width: parent.width
                implicitHeight: contentHeight
                height: Math.min(
                    contentHeight,
                    Theme.volumeStreamsMaxHeight
                )
                model: Audio.streamsModel
                spacing: Theme.spaceXs
                clip: true
                interactive: contentHeight > height

                delegate: Rectangle {
                    required property string app_name
                    required property string title
                    required property int volume_pct
                    required property bool muted
                    required property int stream_id

                    readonly property real _volume: volume_pct / 100.0

                    width: streamList.width
                    radius: Theme.radiusSm
                    color: Theme.bgSurfaceRaised
                    border.color: Theme.borderSubtle
                    border.width: 1
                    implicitHeight: streamCol.implicitHeight + Theme.spaceSm * 2

                    Column {
                        id: streamCol
                        anchors.fill: parent
                        anchors.margins: Theme.spaceSm
                        spacing: Theme.spaceSm

                        RowLayout {
                            width: parent.width

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2

                                Text {
                                    text: app_name || "Unknown app"
                                    color: Theme.fgPrimary
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontBody
                                    font.weight: Theme.weightMedium
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }

                                Text {
                                    visible: root._streamSubtitle({ app_name: app_name, title: title }) !== ""
                                    text: root._streamSubtitle({ app_name: app_name, title: title })
                                    color: Theme.fgMuted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSmall
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                            }

                            Text {
                                id: streamPercent
                                text: root._percentText(_volume)
                                color: muted ? Theme.fgMuted : root._accentFor(_volume)
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.weight: Theme.weightMedium
                                font.features: { "tnum": 1 }

                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: Audio.setStreamVolume(stream_id, 1.0)
                                }
                            }
                        }

                        AudioSlider {
                            width: parent.width
                            modelValue: volume_pct / 100.0
                            accentColor: root._accentFor(liveValue)
                            muted: muted
                            onAdjusted: function(value) {
                                Audio.setStreamVolume(stream_id, value);
                            }
                        }
                    }
                }
            }
        }
    }

    component AudioSlider: Item {
        id: slider

        property real modelValue: 0
        property real liveValue: modelValue
        property real minimumValue: 0
        property real maximumValue: 1.5
        property color accentColor: Theme.accentBlue
        property bool muted: false
        property int updateIntervalMs: 50

        readonly property real _range: Math.max(maximumValue - minimumValue, 0.001)
        readonly property real _ratio: Math.max(
            0,
            Math.min(1, (liveValue - minimumValue) / _range)
        )

        signal adjusted(real value)

        implicitWidth: 200
        implicitHeight: Theme.spaceMd

        onModelValueChanged: {
            if (!dragArea.pressed)
                liveValue = modelValue;
        }

        function _setFromX(x) {
            if (track.width <= 0) return;
            var ratio = Math.max(0, Math.min(1, x / track.width));
            liveValue = minimumValue + (_range * ratio);
        }

        function _commitAdjusted() {
            var pct = Math.round(liveValue * 100);
            if (pct === dragArea.lastSentPct) return;
            dragArea.lastSentPct = pct;
            adjusted(liveValue);
        }

        Timer {
            id: dragCommitTimer
            interval: slider.updateIntervalMs
            repeat: true
            running: dragArea.pressed
            onTriggered: slider._commitAdjusted()
        }

        Rectangle {
            id: track
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            height: Theme.spaceXs
            radius: Theme.radiusXs
            color: Theme.borderDefault
        }

        Rectangle {
            anchors.left: track.left
            anchors.verticalCenter: track.verticalCenter
            width: track.width * slider._ratio
            height: track.height
            radius: Theme.radiusXs
            color: slider.muted ? Theme.fgMuted : slider.accentColor
        }

        Rectangle {
            width: Theme.spaceMd
            height: Theme.spaceMd
            radius: Theme.radiusXs
            x: track.x + (slider._ratio * (track.width - width))
            y: (parent.height - height) / 2
            color: slider.muted ? Theme.fgMuted : Theme.fgPrimary
            border.color: slider.muted ? Theme.borderDefault : slider.accentColor
            border.width: 1
        }

        MouseArea {
            id: dragArea

            property int lastSentPct: -1

            anchors.fill: parent
            hoverEnabled: true
            preventStealing: true
            cursorShape: Qt.PointingHandCursor

            onPressed: function(mouse) {
                slider._setFromX(mouse.x);
                slider._commitAdjusted();
                mouse.accepted = true;
            }

            onPositionChanged: function(mouse) {
                if (!pressed) return;
                slider._setFromX(mouse.x);
                mouse.accepted = true;
            }

            onReleased: function(mouse) {
                slider._setFromX(mouse.x);
                slider._commitAdjusted();
                lastSentPct = -1;
                mouse.accepted = true;
            }

            onCanceled: {
                lastSentPct = -1;
                liveValue = modelValue;
            }
        }
    }
}
