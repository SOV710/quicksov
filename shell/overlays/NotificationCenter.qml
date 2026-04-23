// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property string dragPhase: "idle"
    property int leaderIndex: -1
    property int leaderNotificationId: -1
    property real leaderOffset: 0
    property int pendingDismissId: -1
    property int expandedNotificationId: -1
    property double nowMs: Date.now()

    readonly property bool directFollowActive: dragPhase === "dragging"
                                             || dragPhase === "dismiss_flyout"
    readonly property bool motionLocked: dragPhase !== "idle"

    width: parent ? parent.width : Theme.notificationPanelWidth
    implicitHeight: Math.min(
        contentCol.implicitHeight + Theme.spaceMd * 2,
        Theme.notificationPanelMaxHeight
    )

    function _beginCancelSettle(notificationId, cardIndex) {
        if (!root._isLeader(notificationId, cardIndex) || root.dragPhase !== "dragging") return;

        root.dragPhase = "cancel_settling";
        root.leaderOffset = 0;
        settleTimer.interval = Theme.statusDockRevealDuration;
        settleTimer.restart();
    }

    function _beginDetachSettle(notificationId, cardIndex) {
        if (!root._isLeader(notificationId, cardIndex) || root.dragPhase !== "dismiss_flyout") return;
        if (root.pendingDismissId !== notificationId) return;

        root.dragPhase = "detach_settling";
        root.leaderOffset = 0;
        settleTimer.interval = Theme.statusDockRevealDuration;
        settleTimer.restart();
    }

    function _beginDismissFlyout(notificationId, cardIndex) {
        if (!root._isLeader(notificationId, cardIndex) || root.dragPhase !== "dragging") return;

        root.dragPhase = "dismiss_flyout";
        root.pendingDismissId = notificationId;
    }

    function _enterDragging(notificationId, cardIndex) {
        settleTimer.stop();
        root.dragPhase = "dragging";
        root.leaderIndex = cardIndex;
        root.leaderNotificationId = notificationId;
        root.leaderOffset = 0;
        root.pendingDismissId = -1;
    }

    function _hasNotification(id) {
        if (id < 0 || !Notification.notifications) return false;

        for (var i = 0; i < Notification.notifications.length; ++i) {
            var notif = Notification.notifications[i];
            if (notif && notif.id === id)
                return true;
        }

        return false;
    }

    function _hasExpandedNotification() {
        if (root.expandedNotificationId < 0 || !Notification.notifications) return false;

        for (var i = 0; i < Notification.notifications.length; ++i) {
            var notif = Notification.notifications[i];
            if (notif && notif.id === root.expandedNotificationId)
                return true;
        }

        return false;
    }

    function _isLeader(notificationId, cardIndex) {
        return root.leaderNotificationId === notificationId && root.leaderIndex === cardIndex;
    }

    function _markVisibleAsRead() {
        if (!root.visible || !Notification.connected || !Notification.hasUnread) return;
        Notification.markRead();
    }

    function _neighborOffsetForIndex(cardIndex) {
        if (!root.directFollowActive || Math.abs(cardIndex - root.leaderIndex) !== 1) return 0;

        var maxPull = Theme.spaceXl + Theme.spaceSm;
        return maxPull * (1 - Math.exp(-root.leaderOffset / 52));
    }

    function _pruneTransientState() {
        if (!root._hasExpandedNotification())
            root.expandedNotificationId = -1;
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

    function _resetMotionState() {
        settleTimer.stop();
        root.dragPhase = "idle";
        root.leaderIndex = -1;
        root.leaderNotificationId = -1;
        root.leaderOffset = 0;
        root.pendingDismissId = -1;
    }

    function _updateLeaderOffset(notificationId, cardIndex, offset) {
        if (!root.directFollowActive || !root._isLeader(notificationId, cardIndex)) return;
        root.leaderOffset = Math.max(0, offset);
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
            root._resetMotionState();
        }
    }

    Connections {
        target: Notification

        function onCountChanged() {
            root._markVisibleAsRead();
        }

        function onNotificationsChanged() {
            if (root.dragPhase === "dragging" || root.dragPhase === "dismiss_flyout") {
                root._resetMotionState();
            } else if (root.dragPhase !== "idle" && !root._hasNotification(root.leaderNotificationId)) {
                root._resetMotionState();
            }
            root._pruneTransientState();
        }
    }

    Timer {
        id: settleTimer

        interval: Theme.statusDockRevealDuration
        repeat: false
        running: false

        onTriggered: {
            if (root.dragPhase === "cancel_settling") {
                root._resetMotionState();
                return;
            }

            if (root.dragPhase === "detach_settling") {
                var dismissId = root.pendingDismissId;
                root._resetMotionState();
                if (dismissId >= 0) {
                    if (root.expandedNotificationId === dismissId)
                        root.expandedNotificationId = -1;
                    Notification.dismiss(dismissId);
                }
            }
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
            interactive: !root.motionLocked && contentHeight > height
            spacing: Theme.spaceSm

            delegate: NotificationCard {
                required property int index
                required property var modelData

                cardIndex: index
                directFollowActive: root.directFollowActive
                expanded: root.expandedNotificationId === modelData.id
                motionLocked: root.motionLocked
                neighborOffset: root._neighborOffsetForIndex(index)
                notif: modelData
                relativeTime: root._relativeTimeLabel(modelData ? modelData.timestamp : 0)
                width: notifList.width

                onDismissRequested: notificationId => {
                    if (root.expandedNotificationId === notificationId)
                        root.expandedNotificationId = -1;
                    Notification.dismiss(notificationId);
                }

                onDragStarted: (notificationId, cardIndex) => {
                    root._enterDragging(notificationId, cardIndex);
                }

                onDragOffsetChanged: (notificationId, cardIndex, offset) => {
                    root._updateLeaderOffset(notificationId, cardIndex, offset);
                }

                onCancelReleaseRequested: (notificationId, cardIndex) => {
                    root._beginCancelSettle(notificationId, cardIndex);
                }

                onDismissFlyoutStarted: (notificationId, cardIndex) => {
                    root._beginDismissFlyout(notificationId, cardIndex);
                }

                onDismissFlyoutCompleted: (notificationId, cardIndex) => {
                    root._beginDetachSettle(notificationId, cardIndex);
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
