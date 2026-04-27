// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import ".."
import "../components"
import "../services"

Item {
    id: root

    width: parent ? parent.width : Theme.volumePanelWidth
    implicitHeight: Math.min(
        contentCol.implicitHeight + Theme.spaceMd * 2,
        Theme.volumePanelMaxHeight
    )

    property bool outputsExpanded: false

    readonly property bool _hasAudio: Audio.ready && Audio.defaultSink !== null
    readonly property bool _hasMultipleSinks: Audio.sinks.length > 1

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

    Connections {
        target: Audio

        function onSinksChanged() {
            if (Audio.sinks.length <= 1)
                root.outputsExpanded = false;
        }

        function onReadyChanged() {
            if (!Audio.ready)
                root.outputsExpanded = false;
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

        Rectangle {
            width: parent.width
            radius: Theme.radiusSm
            color: Theme.bgSurfaceRaised
            border.color: Theme.borderSubtle
            border.width: 1
            implicitHeight: masterRow.implicitHeight + Theme.spaceSm * 2

            RowLayout {
                id: masterRow
                anchors.fill: parent
                anchors.margins: Theme.spaceSm
                spacing: Theme.spaceSm

                Rectangle {
                    Layout.preferredWidth: Theme.iconSize + Theme.spaceSm
                    Layout.preferredHeight: Theme.iconSize + Theme.spaceSm
                    radius: Theme.radiusXs
                    color: muteHover.hovered ? Theme.surfaceHover : "transparent"

                    SvgIcon {
                        anchors.centerIn: parent
                        iconPath: Theme.volumeIconFor(Audio.muted, Audio.volume)
                        size: Theme.iconSize
                        color: root._hasAudio
                               ? (Audio.muted ? Theme.fgMuted : root._accentFor(Audio.volume))
                               : Theme.fgMuted
                    }

                    HoverHandler {
                        id: muteHover
                        enabled: Audio.defaultSink !== null
                    }

                    MouseArea {
                        anchors.fill: parent
                        enabled: Audio.defaultSink !== null
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: {
                            if (Audio.defaultSink)
                                Audio.setMuted(Audio.defaultSink.id, !Audio.muted);
                        }
                    }
                }

                AudioSlider {
                    visible: Audio.defaultSink !== null
                    Layout.fillWidth: true
                    modelValue: Audio.defaultSink ? (Audio.defaultSink.volume_pct / 100.0) : 0
                    accentColor: root._accentFor(liveValue)
                    muted: Audio.muted
                    onAdjusted: function(value) {
                        if (Audio.defaultSink)
                            Audio.setVolume(Audio.defaultSink.id, value);
                    }
                }

                Text {
                    id: masterPercent
                    text: root._hasAudio ? root._percentText(Audio.volume) : "—"
                    color: root._hasAudio
                           ? (Audio.muted ? Theme.fgMuted : root._accentFor(Audio.volume))
                           : Theme.fgMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontLabel
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
            }
        }

        Column {
            width: parent.width
            spacing: Theme.spaceXs

            Text {
                text: "Output"
                color: Theme.fgMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                font.weight: Theme.weightMedium
            }

            Rectangle {
                width: parent.width
                radius: Theme.radiusSm
                color: Theme.bgSurfaceRaised
                border.color: root.outputsExpanded ? Theme.borderAccent : Theme.borderSubtle
                border.width: 1
                implicitHeight: outputsCol.implicitHeight + Theme.spaceSm * 2

                Column {
                    id: outputsCol
                    anchors.fill: parent
                    anchors.margins: Theme.spaceSm
                    spacing: Theme.spaceSm

                    Rectangle {
                        width: parent.width
                        radius: Theme.radiusXs
                        color: outputRowMouse.pressed
                               ? Theme.surfaceActive
                               : root.outputsExpanded
                                 ? Theme.surfaceActive
                               : outputHover.hovered
                                 ? Theme.surfaceHover
                                 : "transparent"
                        implicitHeight: currentOutputRow.implicitHeight + Theme.spaceXs * 2

                        RowLayout {
                            id: currentOutputRow
                            anchors.fill: parent
                            anchors.margins: Theme.spaceXs
                            spacing: Theme.spaceSm

                            SvgIcon {
                                iconPath: Theme.iconSpeakerStatus
                                size: Theme.iconSize
                                color: Theme.fgSecondary
                            }

                            Text {
                                text: root._sinkLabel(Audio.defaultSink)
                                color: Theme.fgPrimary
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.weight: Theme.weightMedium
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }

                            SvgIcon {
                                visible: root._hasMultipleSinks
                                iconPath: root.outputsExpanded
                                          ? Theme.iconKeyboardArrowUpStatus
                                          : Theme.iconKeyboardArrowDownStatus
                                size: Theme.iconSize
                                color: Theme.fgMuted
                            }
                        }

                        HoverHandler {
                            id: outputHover
                            enabled: root._hasMultipleSinks
                        }

                        MouseArea {
                            id: outputRowMouse
                            anchors.fill: parent
                            enabled: root._hasMultipleSinks
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: root.outputsExpanded = !root.outputsExpanded
                        }
                    }

                    Rectangle {
                        visible: root.outputsExpanded && root._hasMultipleSinks
                        width: parent.width
                        height: 1
                        color: Theme.borderSubtle
                    }

                    Column {
                        visible: root.outputsExpanded && root._hasMultipleSinks
                        width: parent.width
                        spacing: Theme.spaceXs

                        Repeater {
                            model: Audio.sinks

                            delegate: Rectangle {
                                id: sinkDelegate
                                required property var modelData

                                readonly property bool _isCurrent: Audio.defaultSink && Audio.defaultSink.id === modelData.id

                                width: parent.width
                                radius: Theme.radiusXs
                                color: _isCurrent
                                       ? Theme.surfaceActive
                                       : sinkHover.hovered
                                         ? Theme.surfaceHover
                                         : "transparent"
                                implicitHeight: sinkRow.implicitHeight + Theme.spaceXs * 2

                                RowLayout {
                                    id: sinkRow
                                    anchors.fill: parent
                                    anchors.margins: Theme.spaceXs
                                    spacing: Theme.spaceSm

                                    SvgIcon {
                                        iconPath: sinkDelegate._isCurrent
                                                  ? Theme.iconRadioButtonCheckedStatus
                                                  : Theme.iconRadioButtonUncheckedStatus
                                        size: Theme.iconSize
                                        color: sinkDelegate._isCurrent ? Theme.accentBlue : Theme.fgMuted
                                    }

                                    Text {
                                        text: root._sinkLabel(sinkDelegate.modelData)
                                        color: Theme.fgPrimary
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontBody
                                        font.weight: sinkDelegate._isCurrent ? Theme.weightMedium : Theme.weightRegular
                                        elide: Text.ElideRight
                                        Layout.fillWidth: true
                                    }
                                }

                                HoverHandler {
                                    id: sinkHover
                                }

                                MouseArea {
                                    id: sinkMouse
                                    anchors.fill: parent
                                    enabled: !sinkDelegate._isCurrent
                                    hoverEnabled: true
                                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                                    onClicked: Audio.setDefaultSink(sinkDelegate.modelData.id)
                                }
                            }
                        }
                    }
                }
            }
        }

        Column {
            width: parent.width
            spacing: Theme.spaceSm

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
                    id: streamDelegate
                    required property string app_name
                    required property string title
                    required property string icon
                    required property int volume_pct
                    required property bool muted
                    required property int stream_id

                    readonly property real _volume: volume_pct / 100.0
                    readonly property string _label: app_name || "Playback"

                    width: streamList.width
                    radius: Theme.radiusSm
                    color: Theme.bgSurfaceRaised
                    border.color: Theme.borderSubtle
                    border.width: 1
                    implicitHeight: streamRow.implicitHeight + Theme.spaceSm * 2

                    RowLayout {
                        id: streamRow
                        anchors.fill: parent
                        anchors.margins: Theme.spaceSm
                        spacing: Theme.spaceSm

                        Item {
                            Layout.alignment: Qt.AlignVCenter
                            Layout.preferredWidth: Theme.iconSize + Theme.spaceSm
                            Layout.preferredHeight: Theme.iconSize + Theme.spaceSm

                            Image {
                                anchors.centerIn: parent
                                width: Theme.iconSize + 4
                                height: width
                                asynchronous: true
                                fillMode: Image.PreserveAspectFit
                                mipmap: true
                                smooth: true
                                source: streamDelegate.icon
                                visible: streamDelegate.icon !== "" && status === Image.Ready
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceXs

                            Text {
                                text: streamDelegate._label
                                color: Theme.fgMuted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                                font.weight: Theme.weightMedium
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceSm

                                AudioSlider {
                                    Layout.fillWidth: true
                                    modelValue: streamDelegate.volume_pct / 100.0
                                    accentColor: root._accentFor(liveValue)
                                    muted: streamDelegate.muted
                                    onAdjusted: function(value) {
                                        Audio.setStreamVolume(streamDelegate.stream_id, value);
                                    }
                                }

                                Text {
                                    id: streamPercent
                                    text: root._percentText(streamDelegate._volume)
                                    color: streamDelegate.muted ? Theme.fgMuted : root._accentFor(streamDelegate._volume)
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontBody
                                    font.weight: Theme.weightMedium
                                    font.features: { "tnum": 1 }

                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: Audio.setStreamVolume(streamDelegate.stream_id, 1.0)
                                    }
                                }
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
                slider.liveValue = slider.modelValue;
            }
        }
    }
}
