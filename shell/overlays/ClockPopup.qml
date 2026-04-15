// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Item {
    id: root

    property bool popupVisible: false
    width:  280
    height: popupVisible ? 180 : 0

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

        Column {
            anchors {
                fill: parent
                margins: Theme.spaceMd
            }
            spacing: Theme.spaceSm

            Text {
                text: Qt.formatTime(new Date(), "HH:mm:ss")
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontDisplay
                font.weight: Theme.weightSemibold
                font.features: { "tnum": 1 }

                Timer { interval: 1000; running: root.popupVisible; repeat: true; onTriggered: parent.text = Qt.formatTime(new Date(), "HH:mm:ss") }
            }

            Text {
                text: Qt.formatDate(new Date(), "dddd, MMMM d yyyy")
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
            }
        }
    }
}
