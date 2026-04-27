// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Item {
    id: root

    property string dragPhase: "idle"
    property int leaderIndex: -1
    property int leaderNotificationId: -1
    property real leaderOffset: 0
    property int pendingDismissId: -1
    property int expandedNotificationId: -1
    property string _uiVisibilityKey: "notification-center-" + Math.random().toString(36).slice(2)
    property double nowMs: Date.now()
    readonly property bool directFollowActive: dragPhase === "dragging"
                                             || dragPhase === "dismiss_flyout"
    readonly property bool hasNotifications: Notification.notificationModel.count > 0
    readonly property bool motionLocked: dragPhase !== "idle"
    readonly property bool clearAllEnabled: root.hasNotifications && !root.motionLocked
    readonly property bool revealReady: Notification.ready
    readonly property real measuredNotificationListHeight: notificationMeasureCol.implicitHeight

    width: parent ? parent.width : Theme.notificationPanelWidth
    implicitHeight: Math.min(
        contentCol.implicitHeight + Theme.spaceMd * 2,
        Theme.notificationPanelMaxHeight
    )

    function _beginCancelSettle(notificationId, cardIndex) {
        if (!root._isLeader(notificationId, cardIndex) || root.dragPhase !== "dragging") return;

        root.dragPhase = "cancel_settling";
        root.leaderOffset = 0;
        settleTimer.interval = Theme.motionFast;
        settleTimer.restart();
    }

    function _beginDismissFlyout(notificationId, cardIndex) {
        if (!root._isLeader(notificationId, cardIndex) || root.dragPhase !== "dragging") return;

        root.dragPhase = "dismiss_flyout";
        root.pendingDismissId = notificationId;
    }

    function _completeDismiss(notificationId, cardIndex) {
        if (!root._isLeader(notificationId, cardIndex)) return;
        if (root.pendingDismissId !== notificationId) return;

        if (root.dragPhase !== "dismiss_flyout") return;

        Notification.dismiss(notificationId);
        root._resetMotionState();
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
        return Notification.hasNotification(id);
    }

    function _hasExpandedNotification() {
        return root.expandedNotificationId >= 0
            && Notification.hasNotification(root.expandedNotificationId);
    }

    function _isLeader(notificationId, cardIndex) {
        return root.leaderNotificationId === notificationId && root.leaderIndex === cardIndex;
    }

    function _markVisibleAsRead() {
        if (!root.visible || !Notification.connected || !Notification.hasUnread) return;
        Notification.markRead();
    }

    function _notificationData(notificationId, appName, summary, body, icon, urgency, timestamp) {
        return {
            id: notificationId,
            app_name: appName || "",
            summary: summary || "",
            body: body || "",
            icon: icon || "",
            urgency: urgency || "normal",
            timestamp: timestamp || 0,
            actions: Notification.actionsFor(notificationId)
        };
    }

    function _neighborOffsetForIndex(cardIndex) {
        if (!root.directFollowActive) return 0;

        var distance = Math.abs(cardIndex - root.leaderIndex);
        if (distance === 0) return 0;

        var maxPull = Theme.spaceXl + Theme.spaceSm;
        var basePull = maxPull * (1 - Math.exp(-root.leaderOffset / 52));
        var falloff = Math.pow(0.58, distance - 1);
        return basePull * falloff;
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
        NotificationUiState.setNotificationCenterVisible(root._uiVisibilityKey, root.visible);
    }

    Component.onDestruction: NotificationUiState.setNotificationCenterVisible(root._uiVisibilityKey, false)

    onVisibleChanged: {
        NotificationUiState.setNotificationCenterVisible(root._uiVisibilityKey, visible);
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

        interval: Theme.motionFast
        repeat: false
        running: false

        onTriggered: {
            if (root.dragPhase === "cancel_settling") {
                root._resetMotionState();
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
            width: parent.width
            implicitHeight: clearAllButton.height

            Item {
                id: clearAllButton

                anchors.right: parent.right
                width: Theme.statusIconSize + Theme.spaceMd
                height: width
                opacity: root.clearAllEnabled ? 1 : 0.48

                Rectangle {
                    anchors.fill: parent
                    radius: width / 2
                    color: clearAllMouseArea.containsMouse && root.clearAllEnabled
                           ? Theme.overlay(Theme.chromeSubtleFill, Theme.colorError, 0.16)
                           : "transparent"
                    border.color: root.clearAllEnabled
                                  ? Theme.withAlpha(
                                      clearAllMouseArea.containsMouse ? Theme.colorError : Theme.borderDefault,
                                      clearAllMouseArea.containsMouse ? 0.48 : 0.26
                                  )
                                  : Theme.withAlpha(Theme.borderDefault, 0.12)
                    border.width: 1

                    Behavior on color { ColorAnimation { duration: Theme.motionFast } }
                    Behavior on border.color { ColorAnimation { duration: Theme.motionFast } }
                }

                SvgIcon {
                    anchors.centerIn: parent
                    iconPath: Theme.iconDeleteStatus
                    size: Theme.iconSize
                    color: root.clearAllEnabled
                           ? (clearAllMouseArea.containsMouse ? Theme.colorError : Theme.fgSecondary)
                           : Theme.fgDisabled

                    Behavior on color { ColorAnimation { duration: Theme.motionFast } }
                }

                MouseArea {
                    id: clearAllMouseArea

                    anchors.fill: parent
                    enabled: root.clearAllEnabled
                    hoverEnabled: true
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor

                    onClicked: {
                        root.expandedNotificationId = -1;
                        Notification.dismissAll();
                    }
                }
            }
        }

        Item {
            visible: !root.hasNotifications
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

            visible: root.hasNotifications
            width: parent.width
            implicitHeight: root.measuredNotificationListHeight
            height: Math.min(root.measuredNotificationListHeight, Theme.notificationListMaxHeight)
            model: Notification.notificationModel
            boundsBehavior: Flickable.StopAtBounds
            clip: true
            interactive: !root.motionLocked && root.measuredNotificationListHeight > height
            spacing: Theme.spaceSm

            delegate: NotificationCard {
                required property int index
                required property int notification_id
                required property string app_name
                required property string summary
                required property string body
                required property string icon
                required property string urgency
                required property double timestamp

                property var notifData: root._notificationData(
                    notification_id,
                    app_name,
                    summary,
                    body,
                    icon,
                    urgency,
                    timestamp
                )

                cardIndex: index
                directFollowActive: root.directFollowActive
                expanded: root.expandedNotificationId === notification_id
                motionLocked: root.motionLocked
                neighborOffset: root._neighborOffsetForIndex(index)
                notif: notifData
                relativeTime: root._relativeTimeLabel(timestamp)
                width: notifList.width

                onDismissRequested: notificationId => {
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
                    root._completeDismiss(notificationId, cardIndex);
                }

                onToggleExpandedRequested: notificationId => {
                    root.expandedNotificationId = root.expandedNotificationId === notificationId
                                                ? -1
                                                : notificationId;
                }
            }
        }
    }

    Item {
        id: measurementHost

        x: -width - Theme.notificationPanelWidth
        y: 0
        width: contentCol.width
        height: notificationMeasureCol.implicitHeight
        enabled: false
        opacity: 0
        visible: Notification.notificationModel.count > 0

        Column {
            id: notificationMeasureCol

            width: parent.width
            spacing: Theme.spaceSm

            Repeater {
                model: Notification.notificationModel

                delegate: Item {
                    required property int notification_id
                    required property string app_name
                    required property string summary
                    required property string body
                    required property string icon
                    required property string urgency
                    required property double timestamp

                    property var notifData: root._notificationData(
                        notification_id,
                        app_name,
                        summary,
                        body,
                        icon,
                        urgency,
                        timestamp
                    )

                    width: notificationMeasureCol.width
                    implicitHeight: cardContent.implicitHeight + Theme.spaceMd * 2
                    height: implicitHeight

                    NotificationCardContent {
                        id: cardContent

                        anchors.fill: parent
                        anchors.margins: Theme.spaceMd
                        expanded: root.expandedNotificationId === notification_id
                        interactive: false
                        notif: notifData
                        relativeTime: root._relativeTimeLabel(timestamp)
                    }
                }
            }
        }
    }
}
