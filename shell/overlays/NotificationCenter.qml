// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Layouts
import ".."
import "../components"
import "../services"

Rectangle {
    id: root

    width:  340
    implicitHeight: height
    height: Math.min(contentCol.implicitHeight + Theme.spaceMd * 2, 520)
    radius: Theme.radiusMd
    color:  Theme.bgSurface
    border.color: Theme.borderDefault
    border.width: 1
    opacity: visible ? Theme.opacityPopup : 0
    clip: true

    Behavior on opacity { NumberAnimation { duration: Theme.motionFast } }

    // ── helpers ─────────────────────────────────────────────────────────────
    function _timeLabel(ts) {
        if (!ts) return "";
        var d = new Date(ts);
        var h = d.getHours().toString().padStart(2, "0");
        var m = d.getMinutes().toString().padStart(2, "0");
        return h + ":" + m;
    }

    function _urgencyColor(urgency) {
        if (urgency === "critical") return Theme.colorError;
        if (urgency === "low")      return Theme.fgMuted;
        return Theme.fgSecondary;
    }

    // ── layout ───────────────────────────────────────────────────────────────
    Column {
        id: contentCol
        anchors {
            top: parent.top; left: parent.left; right: parent.right
            margins: Theme.spaceMd
        }
        spacing: Theme.spaceSm

        // header row
        RowLayout {
            width: parent.width

            Text {
                text: "Notifications"
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontLabel
                font.weight: Theme.weightSemibold
                Layout.fillWidth: true
            }

            Text {
                visible: Notification.notifications.length > 0
                text: "Clear all"
                color: Theme.accentBlue
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Notification.dismissAll()
                }
            }
        }

        // empty state
        Item {
            visible: Notification.notifications.length === 0
            width: parent.width
            implicitHeight: 80

            Text {
                anchors.centerIn: parent
                text: "No notifications"
                color: Theme.fgMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                horizontalAlignment: Text.AlignHCenter
            }
        }

        // notification cards
        ListView {
            id: notifList
            visible: Notification.notifications.length > 0
            width:  parent.width
            implicitHeight: contentHeight
            height: Math.min(contentHeight, 440)
            model:  Notification.notifications
            clip:   true
            spacing: Theme.spaceXs
            interactive: contentHeight > height

            delegate: NotifCard {
                required property var modelData
                notif: modelData
                width: notifList.width
            }
        }
    }

    // ── notification card component ──────────────────────────────────────────
    component NotifCard: Rectangle {
        id: card
        property var notif: null
        property bool expanded: false

        readonly property color _accent: root._urgencyColor(notif ? notif.urgency : "normal")

        radius: Theme.radiusSm
        color: cardHover.containsMouse ? Theme.surfaceHover : Qt.rgba(1,1,1,0.04)
        border.color: notif && notif.urgency === "critical"
                      ? Qt.rgba(Theme.colorError.r, Theme.colorError.g, Theme.colorError.b, 0.5)
                      : Theme.borderSubtle
        border.width: 1
        implicitHeight: cardCol.implicitHeight + Theme.spaceXs * 2
        clip: true

        HoverHandler { id: cardHover }

        Behavior on color { ColorAnimation { duration: Theme.motionFast } }

        // urgency accent bar on the left
        Rectangle {
            width: 3
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom; topMargin: 4; bottomMargin: 4 }
            radius: 2
            color: card._accent
            visible: notif && notif.urgency !== "normal"
        }

        Column {
            id: cardCol
            anchors {
                left: parent.left; right: parent.right; top: parent.top
                margins: Theme.spaceXs
                leftMargin: notif && notif.urgency !== "normal" ? Theme.spaceXs + 6 : Theme.spaceXs
            }
            spacing: 2

            // app name + time + dismiss
            RowLayout {
                width: parent.width

                Text {
                    text: notif ? notif.app_name : ""
                    color: card._accent
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    font.weight: Theme.weightMedium
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }

                Text {
                    text: root._timeLabel(notif ? notif.timestamp : 0)
                    color: Theme.fgMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    visible: notif && notif.timestamp > 0
                }

                SvgIcon {
                    iconPath: "lucide/x.svg"
                    size: Theme.fontSmall
                    color: Theme.fgMuted
                    Layout.leftMargin: Theme.spaceXs

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: if (notif) Notification.dismiss(notif.id)
                    }
                }
            }

            // summary
            Text {
                text: notif ? notif.summary : ""
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Theme.weightMedium
                elide: Text.ElideRight
                width: parent.width
            }

            // body
            Text {
                id: bodyLabel
                visible: notif && notif.body !== ""
                text: notif ? notif.body : ""
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                wrapMode: Text.WordWrap
                maximumLineCount: card.expanded ? 0 : 3
                elide: card.expanded ? Text.ElideNone : Text.ElideRight
                width: parent.width
            }

            Text {
                id: bodyMeasure
                visible: false
                text: notif ? notif.body : ""
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                wrapMode: Text.WordWrap
                width: parent.width
            }

            Text {
                visible: bodyLabel.visible && bodyMeasure.lineCount > 3
                text: card.expanded ? "Less" : "More"
                color: Theme.accentBlue
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Theme.weightMedium

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: card.expanded = !card.expanded
                }
            }

            // action buttons
            Row {
                visible: notif && notif.actions && notif.actions.length > 0
                spacing: Theme.spaceXs
                topPadding: 2

                Repeater {
                    model: notif ? notif.actions : []

                    delegate: Rectangle {
                        required property var modelData
                        height: 18
                        width:  actionLabel.implicitWidth + Theme.spaceSm * 2
                        radius: Theme.radiusXs
                        color: actionBtn.containsMouse ? Theme.surfaceActive : Theme.borderDefault

                        Text {
                            id: actionLabel
                            anchors.centerIn: parent
                            text: modelData.label || ""
                            color: Theme.fgPrimary
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody
                        }

                        HoverHandler { id: actionBtn }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: Notification.invokeAction(notif.id, modelData.id)
                        }
                    }
                }
            }
        }
    }
}
