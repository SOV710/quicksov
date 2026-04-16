// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property bool popupVisible: false
    width:  280
    height: popupVisible ? 180 : 0
    implicitHeight: height

    Behavior on height { NumberAnimation { duration: Theme.motionFast; easing.type: Easing.OutCubic } }

    Rectangle {
        width:  280
        height: 180
        radius: Theme.radiusMd
        color:  Theme.bgSurface
        border.color: Theme.borderDefault
        border.width: 1
        opacity: root.popupVisible ? Theme.opacityPopup : 0

        Behavior on opacity { NumberAnimation { duration: Theme.motionFast } }

        // Consume clicks inside the popup so they do not fall through to
        // MainBar's outside-click dismiss layer.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onClicked: function(mouse) { mouse.accepted = true; }
            onPressed: function(mouse) { mouse.accepted = true; }
        }

        Column {
            anchors {
                fill: parent
                margins: Theme.spaceMd
            }
            spacing: Theme.spaceSm

            // Seconds display — reads from the shared Time singleton so it is
            // always in sync with the bar clock with zero startup lag.
            Text {
                text: Qt.formatTime(Time.now, "HH:mm:ss")
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontDisplay
                font.weight: Theme.weightSemibold
                font.features: { "tnum": 1 }
            }

            Text {
                text: Qt.formatDate(Time.now, "dddd, MMMM d yyyy")
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
            }
        }
    }
}
