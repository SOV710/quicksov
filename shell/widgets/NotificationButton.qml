// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Item {
    id: root

    implicitWidth: Theme.statusCapsuleSlotWidth
    implicitHeight: Theme.statusCapsuleHeight

    signal toggled()

    SvgIcon {
        anchors.centerIn: parent
        iconPath: Theme.iconNotificationStatus
        size: Theme.iconSize
        color: Theme.fgPrimary
    }

    Rectangle {
        visible: Notification.hasUnread
        width: 8
        height: 8
        radius: 4
        color: Theme.colorError
        anchors {
            right: parent.right
            rightMargin: 4
            top: parent.top
            topMargin: 10
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.toggled()
    }
}
