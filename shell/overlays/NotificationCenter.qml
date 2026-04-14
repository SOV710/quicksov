// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Layouts
import ".."
import "../services"

Rectangle {
    id: root

    width:  320
    height: Math.min(notifList.contentHeight + Theme.spaceMd * 2 + headerRow.height + Theme.spaceSm, 480)
    radius: Theme.radiusMd
    color:  Theme.bgSurface
    border.color: Theme.borderDefault
    border.width: 1
    opacity: visible ? Theme.opacityPopup : 0
    clip: true

    Behavior on opacity { NumberAnimation { duration: Theme.motionFast } }

    Column {
        anchors {
            fill: parent
            margins: Theme.spaceMd
        }
        spacing: Theme.spaceSm

        RowLayout {
            id: headerRow
            width: parent.width

            Text {
                text: "Notifications"
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontLabel
                font.weight: Theme.weightSemibold
            }

            Item { Layout.fillWidth: true; height: 1 }

            Text {
                text: "Clear all"
                color: Theme.accentBlue
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Notification.dismissAll()
                }
            }
        }

        ListView {
            id: notifList
            width:  parent.width
            height: Math.min(contentHeight, 400)
            model:  Notification.notifications
            clip:   true
            spacing: Theme.spaceXs

            delegate: NotifItem {
                required property var modelData
                notif: modelData
                width: notifList.width
            }
        }

        Text {
            visible: Notification.notifications.length === 0
            text: "No notifications"
            color: Theme.fgMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
        }
    }

    component NotifItem: Rectangle {
        property var notif: null
        height: itemCol.implicitHeight + Theme.spaceSm * 2
        radius: Theme.radiusSm
        color: Theme.surfaceHover

        Column {
            id: itemCol
            anchors {
                left: parent.left; right: parent.right
                top: parent.top
                margins: Theme.spaceXs
            }
            spacing: 2

            Text {
                text: notif ? notif.summary : ""
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                font.weight: Theme.weightMedium
                elide: Text.ElideRight
                width: parent.width - 20
            }

            Text {
                visible: notif && notif.body !== ""
                text: notif ? notif.body : ""
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontMicro
                elide: Text.ElideRight
                maximumLineCount: 2
                wrapMode: Text.WordWrap
                width: parent.width - 20
            }
        }

        Text {
            text: "✕"
            color: Theme.fgMuted
            font.pixelSize: Theme.fontMicro
            anchors { right: parent.right; rightMargin: Theme.spaceXs; verticalCenter: parent.verticalCenter }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: if (notif) Notification.dismiss(notif.id)
            }
        }
    }
}
