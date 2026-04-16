// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Item {
    id: root

    implicitWidth: row.implicitWidth + Theme.spaceSm
    implicitHeight: row.implicitHeight

    signal toggled()

    Row {
        id: row
        spacing: Theme.spaceXs
        anchors.verticalCenter: parent.verticalCenter

        SvgIcon {
            iconPath: "lucide/bell.svg"
            size: Theme.iconSize
            color: Notification.hasUnread ? Theme.colorInfo : Theme.fgMuted
            anchors.verticalCenter: parent.verticalCenter
        }

        Text {
            visible: Notification.count > 0
            text: String(Notification.count)
            color: Notification.hasUnread ? Theme.colorInfo : Theme.fgMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.features: { "tnum": 1 }
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.toggled()
    }
}
