// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property var notif: null
    property bool expanded: false
    property bool pauseAll: false
    property string relativeTime: ""
    property int timerRevision: 0

    readonly property int notificationId: notif && notif.id !== undefined ? notif.id : -1
    readonly property int autoDismissMs: _autoDismissDuration()
    readonly property bool countdownPaused: root.pauseAll || root.expanded

    property int remainingMs: 0
    property double countdownStartedAtMs: 0

    signal toggleExpandedRequested(int notificationId)

    implicitHeight: cardFrame.implicitHeight
    height: implicitHeight
    width: parent ? parent.width : 0

    function _autoDismissDuration() {
        if (!root.notif) return 0;

        switch (root.notif.urgency) {
        case "low":
            return 3000;
        case "normal":
            return 5000;
        default:
            return 0;
        }
    }

    function _expireToast() {
        expiryTimer.stop();
        root.countdownStartedAtMs = 0;
        if (root.notificationId >= 0)
            NotificationUiState.dismissToastPreview(root.notificationId);
    }

    function _pauseCountdown() {
        if (!expiryTimer.running) return;

        var elapsed = Math.max(0, Date.now() - root.countdownStartedAtMs);
        root.remainingMs = Math.max(0, root.remainingMs - elapsed);
        root.countdownStartedAtMs = 0;
        expiryTimer.stop();
    }

    function _resetCountdown() {
        expiryTimer.stop();
        root.countdownStartedAtMs = 0;
        root.remainingMs = root.autoDismissMs;
        root._syncCountdown();
    }

    function _resumeCountdown() {
        if (root.remainingMs <= 0) {
            root._expireToast();
            return;
        }

        root.countdownStartedAtMs = Date.now();
        expiryTimer.interval = root.remainingMs;
        expiryTimer.restart();
    }

    function _syncCountdown() {
        if (root.autoDismissMs <= 0) {
            expiryTimer.stop();
            root.countdownStartedAtMs = 0;
            return;
        }

        if (root.countdownPaused) {
            root._pauseCountdown();
            return;
        }

        if (!expiryTimer.running)
            root._resumeCountdown();
    }

    Component.onCompleted: root._resetCountdown()

    onCountdownPausedChanged: root._syncCountdown()
    onExpandedChanged: root._syncCountdown()
    onPauseAllChanged: root._syncCountdown()
    onTimerRevisionChanged: root._resetCountdown()
    onNotificationIdChanged: root._resetCountdown()

    Behavior on height {
        NumberAnimation {
            duration: Theme.motionNormal
            easing.type: Easing.OutCubic
        }
    }

    Timer {
        id: expiryTimer

        interval: root.remainingMs
        repeat: false
        running: false
        onTriggered: root._expireToast()
    }

    Rectangle {
        id: cardFrame

        width: root.width
        implicitHeight: cardContent.implicitHeight + Theme.spaceMd * 2
        radius: Theme.radiusMd
        color: cardHover.hovered ? Theme.surfaceHover : Theme.chromeSubtleFill
        border.color: root.notif && root.notif.urgency === "critical"
                      ? Theme.dangerBorderSoft
                      : (root.expanded ? Theme.borderDefault : Theme.borderSubtle)
        border.width: 1
        clip: true

        Behavior on color {
            ColorAnimation {
                duration: Theme.motionFast
            }
        }

        HoverHandler {
            id: cardHover
        }

        NotificationCardContent {
            id: cardContent

            anchors.fill: parent
            anchors.margins: Theme.spaceMd
            expanded: root.expanded
            interactive: true
            notif: root.notif
            relativeTime: root.relativeTime

            onActionRequested: actionId => {
                if (root.notif)
                    Notification.invokeActionAndDismiss(root.notif.id, actionId);
            }

            onDismissRequested: {
                if (root.notif)
                    Notification.dismiss(root.notif.id);
            }
        }

        HoverHandler {
            parent: cardContent.summaryArea
            cursorShape: Qt.PointingHandCursor
        }

        TapHandler {
            parent: cardContent.summaryArea
            onTapped: {
                if (root.notificationId >= 0)
                    root.toggleExpandedRequested(root.notificationId);
            }
        }
    }
}
