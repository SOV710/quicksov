// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    implicitWidth: row.implicitWidth
    implicitHeight: row.implicitHeight

    signal openPopup()

    property string _dateText: _formatDate(Time.now)
    property string _timeText: Qt.formatTime(Time.now, "HH:mm")
    property string _weekdayText: Qt.formatDate(Time.now, "ddd")

    function _formatDate(d) {
        return Qt.formatDate(d, "MM/dd");
    }

    Connections {
        target: Time
        function onNowChanged() {
            root._dateText = root._formatDate(Time.now);
            root._timeText = Qt.formatTime(Time.now, "HH:mm");
            root._weekdayText = Qt.formatDate(Time.now, "ddd");
        }
    }

    Row {
        id: row
        spacing: Theme.spaceXs
        anchors.centerIn: parent

        ClockSegment {
            text: root._dateText
            fillColor: Theme.clockDateFill
            textColor: Theme.fgSecondary
            weight: Theme.weightMedium
            pixelSize: Theme.fontSmall
        }

        ClockSegment {
            text: root._timeText
            fillColor: Theme.clockTimeFill
            textColor: Theme.fgPrimary
            weight: Theme.weightSemibold
            pixelSize: Theme.fontBody
        }

        ClockSegment {
            text: root._weekdayText
            fillColor: Theme.clockDayFill
            textColor: Theme.fgPrimary
            weight: Theme.weightMedium
            pixelSize: Theme.fontSmall
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

        implicitWidth: label.implicitWidth + Theme.groupContainerPadX * 2
        implicitHeight: Theme.groupContainerHeight
        radius: Theme.groupContainerRadius
        color: fillColor
        border.color: Theme.withAlpha(textColor, 0.12)
        border.width: 1

        Text {
            id: label
            anchors.centerIn: parent
            text: segment.text
            color: segment.textColor
            font.family: Theme.fontFamily
            font.pixelSize: segment.pixelSize
            font.weight: segment.weight
            font.features: { "tnum": 1 }
        }
    }
}
