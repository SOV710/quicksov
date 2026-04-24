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
    property int cardIndex: -1
    property bool collapsedOut: false
    property bool directFollowActive: false
    property bool motionLocked: false
    property real neighborOffset: 0
    property string relativeTime: ""

    signal dismissRequested(int notificationId)
    signal dragStarted(int notificationId, int cardIndex)
    signal dragOffsetChanged(int notificationId, int cardIndex, real offset)
    signal cancelReleaseRequested(int notificationId, int cardIndex)
    signal dismissFlyoutStarted(int notificationId, int cardIndex)
    signal dismissFlyoutCompleted(int notificationId, int cardIndex)
    signal toggleExpandedRequested(int notificationId)

    readonly property real dismissThreshold: Math.max(80, width * 0.28)

    property bool dismissing: false
    property real dragStartOffset: 0
    property real swipeOffset: 0

    Behavior on neighborOffset {
        enabled: !root.directFollowActive

        SpringAnimation {
            spring: 4.2
            damping: 0.24
            mass: 0.8
            epsilon: 0.1
        }
    }

    Behavior on swipeOffset {
        enabled: !dragHandler.active && !root.dismissing

        SpringAnimation {
            spring: 3.4
            damping: 0.28
            mass: 0.9
            epsilon: 0.1
        }
    }

    implicitHeight: cardFrame.implicitHeight
    height: root.collapsedOut ? 0 : implicitHeight
    width: parent ? parent.width : 0

    Behavior on height {
        NumberAnimation {
            duration: Theme.statusDockRevealDuration
            easing.type: Easing.OutCubic
        }
    }

    function _endDrag() {
        var id = root.notif ? root.notif.id : -1;
        var shouldDismiss = root.swipeOffset >= root.dismissThreshold;

        if (shouldDismiss) {
            root.dismissing = true;
            if (root.notif)
                root.dismissFlyoutStarted(root.notif.id, root.cardIndex);
            dismissAnimation.from = root.swipeOffset;
            dismissAnimation.to = root.width + Theme.spaceXl;
            dismissAnimation.start();
            return;
        }

        if (root.notif)
            root.cancelReleaseRequested(root.notif.id, root.cardIndex);
        root.swipeOffset = 0;
    }

    Rectangle {
        id: cardFrame

        width: root.width
        height: root.height
        x: root.swipeOffset + root.neighborOffset
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

        onXChanged: {
            if ((dragHandler.active || root.dismissing) && root.notif)
                root.dragOffsetChanged(root.notif.id, root.cardIndex, root.swipeOffset);
        }

        NotificationCardContent {
            id: cardContent

            anchors.fill: parent
            anchors.margins: Theme.spaceMd
            expanded: root.expanded
            interactive: !root.motionLocked && !root.dismissing
            notif: root.notif
            relativeTime: root.relativeTime

            onActionRequested: actionId => {
                if (root.notif)
                    Notification.invokeAction(root.notif.id, actionId);
            }

            onDismissRequested: {
                if (root.notif)
                    root.dismissRequested(root.notif.id);
            }
        }

        HoverHandler {
            parent: cardContent.summaryArea
            cursorShape: root.motionLocked ? Qt.ArrowCursor : Qt.PointingHandCursor
        }

        TapHandler {
            parent: cardContent.summaryArea
            enabled: !root.motionLocked && !root.dismissing
            onPressedChanged: {
                if (pressed)
                    cardContent.summaryArea.tapBlockedForCurrentGesture = false;
            }
            onTapped: {
                if (cardContent.summaryArea.tapBlockedForCurrentGesture)
                    return;
                if (root.notif)
                    root.toggleExpandedRequested(root.notif.id);
            }
        }

        DragHandler {
            id: dragHandler

            parent: cardContent.summaryArea
            enabled: !root.motionLocked || active
            target: null
            xAxis.enabled: true
            yAxis.enabled: false

            onActiveChanged: {
                if (active) {
                    cardContent.summaryArea.tapBlockedForCurrentGesture = true;
                    root.dragStartOffset = root.swipeOffset;
                    if (root.notif) {
                        root.dragStarted(root.notif.id, root.cardIndex);
                        root.dragOffsetChanged(root.notif.id, root.cardIndex, root.swipeOffset);
                    }
                    return;
                }

                if (!root.dismissing)
                    root._endDrag();
            }

            onTranslationChanged: {
                if (!active) return;

                var rawOffset = Math.max(0, root.dragStartOffset + translation.x);
                if (rawOffset > root.dismissThreshold)
                    rawOffset = root.dismissThreshold + (rawOffset - root.dismissThreshold) * 0.32;
                root.swipeOffset = rawOffset;
            }
        }
    }

    NumberAnimation {
        id: dismissAnimation

        target: root
        property: "swipeOffset"
        duration: Theme.motionNormal
        easing.type: Easing.OutCubic
        onStopped: {
            if (root.dismissing && root.notif) {
                root.dismissing = false;
                root.dismissFlyoutCompleted(root.notif.id, root.cardIndex);
            }
        }
    }
}
