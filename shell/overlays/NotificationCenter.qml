// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Layouts
import ".."
import "../components"
import "../services"

Rectangle {
    id: root

    width: Theme.notificationPanelWidth
    implicitHeight: height
    height: Math.min(
        contentCol.implicitHeight + Theme.spaceMd * 2,
        Theme.notificationPanelMaxHeight
    )
    radius: Theme.radiusMd
    color:  Theme.bgSurface
    border.color: Theme.borderDefault
    border.width: 1
    opacity: visible ? Theme.opacityPopup : 0
    clip: true

    Behavior on opacity { NumberAnimation { duration: Theme.motionFast } }

    // Consume background clicks so they do not fall through to MainBar's
    // global outside-click dismiss layer.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onClicked: function(mouse) { mouse.accepted = true; }
        onPressed: function(mouse) { mouse.accepted = true; }
    }

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

    function _previewText(text, limit) {
        if (!text || text.length <= limit) return text || "";

        var cut = text.slice(0, limit);
        var lastSpace = Math.max(cut.lastIndexOf(" "), cut.lastIndexOf("\n"));
        if (lastSpace > Math.floor(limit * 0.65)) cut = cut.slice(0, lastSpace);
        return cut.replace(/\s+$/, "") + "...";
    }

    function _displayActions(actions) {
        if (!actions || !actions.length) return [];

        return actions.filter(function(action) {
            return action
                && typeof action.id === "string"
                && typeof action.label === "string"
                && action.label.trim().length > 0;
        });
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
            height: Math.min(contentHeight, Theme.notificationListMaxHeight)
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
        readonly property int _bodyPreviewLimit: Math.max(
            90,
            Math.floor((cardCol.width / (Theme.fontBody * 0.62)) * 3)
        )
        readonly property string _bodyText: notif ? notif.body || "" : ""
        readonly property bool _bodyCanExpand: _bodyText.length > _bodyPreviewLimit
        readonly property var _actions: root._displayActions(notif ? notif.actions : [])

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
                visible: card._bodyText !== ""
                text: card.expanded
                      ? card._bodyText
                      : root._previewText(card._bodyText, card._bodyPreviewLimit)
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                wrapMode: Text.WordWrap
                width: parent.width
            }

            Text {
                visible: bodyLabel.visible && card._bodyCanExpand
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
                visible: card._actions.length > 0
                spacing: Theme.spaceXs
                topPadding: 2

                Repeater {
                    model: card._actions

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
