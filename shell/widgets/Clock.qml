// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    readonly property int _segmentWidth: Math.max(
        Theme.clockSegmentMinWidth,
        Math.ceil(
            Math.max(
                dateMetrics.width,
                timeMetrics.width,
                weekdayMetrics.width
            ) + Theme.clockSegmentPadX * 2
        )
    )

    implicitWidth: _segmentWidth * 3
    implicitHeight: Theme.clockSegmentHeight

    signal openPopup()

    property string _dateText: _formatDate(Time.now)
    property string _timeText: Qt.formatTime(Time.now, "HH:mm")
    property string _weekdayText: Qt.formatDate(Time.now, "ddd")

    function _formatDate(d) {
        return Qt.formatDate(d, "MM/dd");
    }

    TextMetrics {
        id: dateMetrics
        text: root._dateText
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.weight: Theme.weightMedium
    }

    TextMetrics {
        id: timeMetrics
        text: root._timeText
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontBody
        font.weight: Theme.weightSemibold
    }

    TextMetrics {
        id: weekdayMetrics
        text: root._weekdayText
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.weight: Theme.weightMedium
    }

    Connections {
        target: Time
        function onNowChanged() {
            root._dateText = root._formatDate(Time.now);
            root._timeText = Qt.formatTime(Time.now, "HH:mm");
            root._weekdayText = Qt.formatDate(Time.now, "ddd");
        }
    }

    Rectangle {
        id: shell
        anchors.fill: parent
        radius: Theme.clockSegmentRadius
        color: "transparent"
        border.color: Theme.withAlpha(Theme.borderDefault, 0.16)
        border.width: 1
        antialiasing: true

        Row {
            id: row
            anchors.fill: parent
            spacing: 0

            ClockSegment {
                width: root._segmentWidth
                text: root._dateText
                fillColor: Theme.clockDateFill
                textColor: Theme.clockDateText
                weight: Theme.weightMedium
                pixelSize: Theme.fontSmall
                capLeft: true
            }

            ClockSegment {
                width: root._segmentWidth
                text: root._timeText
                fillColor: Theme.clockTimeFill
                textColor: Theme.clockTimeText
                weight: Theme.weightSemibold
                pixelSize: Theme.fontBody
            }

            ClockSegment {
                width: root._segmentWidth
                text: root._weekdayText
                fillColor: Theme.clockDayFill
                textColor: Theme.clockDayText
                weight: Theme.weightMedium
                pixelSize: Theme.fontSmall
                capRight: true
            }
        }

    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.openPopup()
    }

    component ClockSegment: Rectangle {
        id: segment

        property string text: ""
        property color fillColor: Theme.clockDateFill
        property color textColor: Theme.fgPrimary
        property int weight: Theme.weightRegular
        property int pixelSize: Theme.fontSmall
        property bool capLeft: false
        property bool capRight: false

        implicitWidth: root._segmentWidth
        implicitHeight: Theme.clockSegmentHeight
        radius: capLeft || capRight ? Theme.clockSegmentRadius : 0
        color: fillColor
        antialiasing: true

        Rectangle {
            anchors {
                top: parent.top
                bottom: parent.bottom
                left: capLeft ? undefined : parent.left
                right: capLeft ? parent.right : undefined
            }
            width: parent.width - Theme.clockSegmentRadius
            visible: capLeft || capRight
            color: segment.fillColor
        }

        Text {
            id: label
            anchors.centerIn: parent
            text: segment.text
            color: segment.textColor
            font.family: Theme.fontFamily
            font.pixelSize: segment.pixelSize
            font.weight: segment.weight
            font.features: { "tnum": 1 }
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }
}
