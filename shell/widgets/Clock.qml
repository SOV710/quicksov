// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    readonly property int _contentWidth: Math.ceil(
        dateMetrics.width
        + timeMetrics.width
        + weekdayMetrics.width
        + Theme.clockCapsuleGap * 2
    )

    implicitWidth: _contentWidth + Theme.clockCapsulePadX * 2
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
        color: Theme.clockCapsuleFill
        border.color: Theme.withAlpha(Theme.borderDefault, 0.16)
        border.width: 1
        antialiasing: true

        Row {
            anchors.centerIn: parent
            height: parent.height
            spacing: Theme.clockCapsuleGap

            Item {
                width: dateMetrics.width
                height: parent.height

                Text {
                    anchors.centerIn: parent
                    text: root._dateText
                    color: Theme.clockCapsuleText
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    font.weight: Theme.weightMedium
                    font.features: { "tnum": 1 }
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }

            Item {
                width: timeMetrics.width
                height: parent.height

                Text {
                    anchors.centerIn: parent
                    text: root._timeText
                    color: Theme.clockCapsuleText
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    font.weight: Theme.weightSemibold
                    font.features: { "tnum": 1 }
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }

            Item {
                width: weekdayMetrics.width
                height: parent.height

                Text {
                    anchors.centerIn: parent
                    text: root._weekdayText
                    color: Theme.clockCapsuleText
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    font.weight: Theme.weightMedium
                    font.features: { "tnum": 1 }
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.openPopup()
    }
}
