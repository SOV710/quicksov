// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Item {
    id: root

    implicitWidth: timeText.implicitWidth
    implicitHeight: timeText.implicitHeight

    property string _time: Qt.formatTime(new Date(), "HH:mm")
    property string _date: Qt.formatDate(new Date(), "ddd d")

    Timer {
        interval: 1000
        running: true
        repeat: true
        onTriggered: {
            root._time = Qt.formatTime(new Date(), "HH:mm");
            root._date = Qt.formatDate(new Date(), "ddd d");
        }
    }

    Row {
        spacing: Theme.spaceXs

        Text {
            id: timeText
            text: root._time
            color: Theme.fgPrimary
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            font.weight: Theme.weightMedium
            font.features: { "tnum": 1 }
        }

        Text {
            text: root._date
            color: Theme.fgMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Theme.weightRegular
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: clockPopup.popupVisible = !clockPopup.popupVisible
    }
}
