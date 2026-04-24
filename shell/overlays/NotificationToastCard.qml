// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property var notif: null
    property string toastLifecycleState: ""
    property bool pauseAll: false
    property string relativeTime: ""
    property int timerRevision: 0
    property int toastLifecycleRevision: 0

    property int remainingMs: 0
    property double countdownStartedAtMs: 0
    property real cardOffsetX: root._offscreenX
    property real cardOpacity: 0
    property int _animatedLifecycleRevision: -1
    property bool _componentReady: false

    readonly property int notificationId: notif && notif.id !== undefined ? notif.id : -1
    readonly property int autoDismissMs: _autoDismissDuration()
    readonly property real cardFullHeight: cardContent.implicitHeight + Theme.spaceMd * 2
    readonly property bool countdownPaused: root.pauseAll || root.toastLifecycleState !== "open"
    readonly property real _offscreenX: root.width + Theme.spaceXl

    implicitHeight: root.cardFullHeight
    height: 0
    width: parent ? parent.width : 0

    function _applyOpenVisualState() {
        root.height = root.implicitHeight;
        root.cardOffsetX = 0;
        root.cardOpacity = 1;
    }

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

    function _playCloseAnimation() {
        if (!root._componentReady)
            return;

        enterAnimation.stop();

        closeHeightAnimation.from = root.height;
        closeHeightAnimation.to = 0;
        closeSlideAnimation.from = root.cardOffsetX;
        closeSlideAnimation.to = root._offscreenX;
        closeOpacityAnimation.from = root.cardOpacity;
        closeOpacityAnimation.to = 0;
        root._animatedLifecycleRevision = root.toastLifecycleRevision;
        closeAnimation.restart();
    }

    function _playEnterAnimation() {
        if (!root._componentReady || root.width <= 0)
            return;

        closeAnimation.stop();

        if (root.height <= 0.5)
            root.height = 0;
        if (root.cardOpacity <= 0.01) {
            root.cardOffsetX = root._offscreenX;
            root.cardOpacity = 0;
        }

        enterHeightAnimation.from = root.height;
        enterHeightAnimation.to = root.implicitHeight;
        enterSlideAnimation.from = root.cardOffsetX;
        enterSlideAnimation.to = 0;
        enterOpacityAnimation.from = root.cardOpacity;
        enterOpacityAnimation.to = 1;
        root._animatedLifecycleRevision = root.toastLifecycleRevision;
        enterAnimation.restart();
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

    function _syncLifecycleAnimation() {
        if (!root._componentReady)
            return;

        if (root.toastLifecycleState === "")
            return;

        if (root.toastLifecycleState === "entering") {
            root._playEnterAnimation();
            return;
        }

        if (root.toastLifecycleState === "closing") {
            root._playCloseAnimation();
            return;
        }

        enterAnimation.stop();
        closeAnimation.stop();
        root._applyOpenVisualState();
    }

    Component.onCompleted: {
        root._componentReady = true;
        root._resetCountdown();
        if (root.toastLifecycleState === "entering") {
            Qt.callLater(function() {
                root._syncLifecycleAnimation();
            });
        } else if (root.toastLifecycleState !== "") {
            root._syncLifecycleAnimation();
        }
    }

    onCountdownPausedChanged: root._syncCountdown()
    onImplicitHeightChanged: {
        if (root._componentReady && root.toastLifecycleState !== "closing")
            root.height = root.implicitHeight;
    }
    onToastLifecycleStateChanged: {
        root._syncLifecycleAnimation();
        root._syncCountdown();
    }
    onNotificationIdChanged: root._resetCountdown()
    onPauseAllChanged: root._syncCountdown()
    onTimerRevisionChanged: root._resetCountdown()
    onWidthChanged: {
        if (root._componentReady && root.toastLifecycleState === "entering" && !enterAnimation.running)
            root._syncLifecycleAnimation();
    }

    Timer {
        id: expiryTimer

        interval: root.remainingMs
        repeat: false
        running: false
        onTriggered: root._expireToast()
    }

    Item {
        anchors.fill: parent
        clip: true

        Rectangle {
            id: cardFrame

            x: root.cardOffsetX
            width: root.width
            height: root.cardFullHeight
            implicitHeight: root.cardFullHeight
            opacity: root.cardOpacity
            radius: Theme.radiusMd
            color: cardHover.hovered ? Theme.surfaceHover : Theme.chromeSubtleFill
            border.color: root.notif && root.notif.urgency === "critical"
                          ? Theme.dangerBorderSoft
                          : Theme.borderDefault
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

                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: Theme.spaceMd
                anchors.rightMargin: Theme.spaceMd
                anchors.topMargin: Theme.spaceMd
                expanded: true
                interactive: root.toastLifecycleState !== "closing"
                notif: root.notif
                relativeTime: root.relativeTime
                showChevron: false
                showDismissAction: false

                onActionRequested: actionId => {
                    if (root.notif)
                        NotificationUiState.invokeToastAction(root.notif.id, actionId);
                }
            }

            HoverHandler {
                parent: cardContent.summaryArea
                cursorShape: root.toastLifecycleState === "closing" ? Qt.ArrowCursor : Qt.PointingHandCursor
            }

            TapHandler {
                parent: cardContent.summaryArea
                enabled: root.toastLifecycleState !== "closing"
                onTapped: {
                    if (root.notificationId >= 0)
                        NotificationUiState.dismissToastPreview(root.notificationId);
                }
            }
        }
    }

    SequentialAnimation {
        id: enterAnimation
        running: false

        NumberAnimation {
            id: enterHeightAnimation
            target: root
            property: "height"
            duration: Theme.motionFast
            easing.type: Easing.OutCubic
        }

        ParallelAnimation {
            NumberAnimation {
                id: enterSlideAnimation
                target: root
                property: "cardOffsetX"
                duration: Theme.motionSlow
                easing.type: Easing.OutCubic
            }

            NumberAnimation {
                id: enterOpacityAnimation
                target: root
                property: "cardOpacity"
                duration: Theme.motionNormal
                easing.type: Easing.OutCubic
            }
        }

        onFinished: NotificationUiState.markToastEntered(root.notificationId, root._animatedLifecycleRevision)
    }

    SequentialAnimation {
        id: closeAnimation
        running: false

        ParallelAnimation {
            NumberAnimation {
                id: closeSlideAnimation
                target: root
                property: "cardOffsetX"
                duration: Theme.motionFast
                easing.type: Easing.InCubic
            }

            NumberAnimation {
                id: closeOpacityAnimation
                target: root
                property: "cardOpacity"
                duration: Theme.motionFast
                easing.type: Easing.InCubic
            }
        }

        NumberAnimation {
            id: closeHeightAnimation
            target: root
            property: "height"
            duration: Theme.motionNormal
            easing.type: Easing.OutCubic
        }

        onFinished: NotificationUiState.finalizeToastRemoval(root.notificationId, root._animatedLifecycleRevision)
    }
}
