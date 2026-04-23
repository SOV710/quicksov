// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property bool dragInProgress: false
    property int draggedIndex: -1
    property int draggedNotificationId: -1
    property real dragOffset: 0
    property int expandedNotificationId: -1
    property double nowMs: Date.now()

    width: parent ? parent.width : Theme.notificationPanelWidth
    implicitHeight: Math.min(
        contentCol.implicitHeight + Theme.spaceMd * 2,
        Theme.notificationPanelMaxHeight
    )

    function _clearDragState() {
        root.dragInProgress = false;
        root.draggedIndex = -1;
        root.draggedNotificationId = -1;
        root.dragOffset = 0;
    }

    function _hasNotification(id) {
        if (id < 0 || !Notification.notifications) return false;

        for (var i = 0; i < Notification.notifications.length; ++i) {
            var notif = Notification.notifications[i];
            if (notif && notif.id === id) return true;
        }

        return false;
    }

    function _markVisibleAsRead() {
        if (!root.visible || !Notification.connected || !Notification.hasUnread) return;
        Notification.markRead();
    }

    function _neighborOffsetForIndex(cardIndex) {
        if (!root.dragInProgress || Math.abs(cardIndex - root.draggedIndex) !== 1) return 0;
        var maxPull = Theme.spaceXl + Theme.spaceSm;
        return maxPull * (1 - Math.exp(-root.dragOffset / 52));
    }

    function _pruneTransientState() {
        if (!root._hasNotification(root.expandedNotificationId))
            root.expandedNotificationId = -1;
        if (!root._hasNotification(root.draggedNotificationId))
            root._clearDragState();
    }

    function _relativeTimeLabel(ts) {
        if (!ts) return "";

        var delta = Math.max(0, root.nowMs - ts);
        if (delta < 45000) return "now";

        var minute = 60 * 1000;
        var hour = 60 * minute;
        var day = 24 * hour;

        if (delta < hour) return Math.max(1, Math.floor(delta / minute)) + "m";
        if (delta < day) return Math.max(1, Math.floor(delta / hour)) + "h";
        return Math.max(1, Math.floor(delta / day)) + "d";
    }

    function _setDragOffset(notificationId, cardIndex, offset) {
        if (!root.dragInProgress) return;
        if (root.draggedNotificationId !== notificationId || root.draggedIndex !== cardIndex) return;
        root.dragOffset = Math.max(0, offset);
    }

    function _setDragState(notificationId, cardIndex, active) {
        if (active) {
            root.dragInProgress = true;
            root.draggedNotificationId = notificationId;
            root.draggedIndex = cardIndex;
            return;
        }

        if (root.draggedNotificationId === notificationId || root.draggedIndex === cardIndex)
            root._clearDragState();
    }

    Component.onCompleted: {
        root.nowMs = Date.now();
        root._markVisibleAsRead();
        root._pruneTransientState();
    }

    onVisibleChanged: {
        if (visible) {
            root.nowMs = Date.now();
            root._markVisibleAsRead();
        } else {
            root._clearDragState();
        }
    }

    Connections {
        target: Notification

        function onCountChanged() {
            root._markVisibleAsRead();
        }

        function onNotificationsChanged() {
            root._clearDragState();
            root._pruneTransientState();
        }
    }

    Timer {
        interval: 30000
        repeat: true
        running: root.visible
        onTriggered: root.nowMs = Date.now()
    }

    Column {
        id: contentCol

        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            margins: Theme.spaceMd
        }
        spacing: Theme.spaceSm

        Item {
            visible: Notification.notifications.length === 0
            width: parent.width
            implicitHeight: 72

            Text {
                anchors.centerIn: parent
                color: Theme.fgMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                horizontalAlignment: Text.AlignHCenter
                text: "No notifications"
            }
        }

        ListView {
            id: notifList

            visible: Notification.notifications.length > 0
            width: parent.width
            implicitHeight: contentHeight
            height: Math.min(contentHeight, Theme.notificationListMaxHeight)
            model: Notification.notifications
            boundsBehavior: Flickable.StopAtBounds
            clip: true
            interactive: !root.dragInProgress && contentHeight > height
            spacing: Theme.spaceSm

            delegate: NotificationCard {
                required property int index
                required property var modelData

                cardIndex: index
                dragInProgress: root.dragInProgress
                expanded: root.expandedNotificationId === modelData.id
                neighborOffset: root._neighborOffsetForIndex(index)
                notif: modelData
                relativeTime: root._relativeTimeLabel(modelData ? modelData.timestamp : 0)
                width: notifList.width

                onDismissRequested: notificationId => {
                    if (root.expandedNotificationId === notificationId)
                        root.expandedNotificationId = -1;
                    if (root.draggedNotificationId === notificationId)
                        root._clearDragState();
                    Notification.dismiss(notificationId);
                }

                onDragOffsetChanged: (notificationId, cardIndex, offset) => {
                    root._setDragOffset(notificationId, cardIndex, offset);
                }

                onDragStateChanged: (notificationId, cardIndex, active) => {
                    root._setDragState(notificationId, cardIndex, active);
                }

                onToggleExpandedRequested: notificationId => {
                    root.expandedNotificationId = root.expandedNotificationId === notificationId
                                                ? -1
                                                : notificationId;
                }
            }
        }
    }
}
