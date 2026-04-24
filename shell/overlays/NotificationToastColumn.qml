// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property double nowMs: Date.now()

    readonly property bool pauseAll: columnHover.hovered || toastList.dragging

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

    HoverHandler {
        id: columnHover
    }

    Timer {
        interval: 30000
        repeat: true
        running: root.visible
        onTriggered: root.nowMs = Date.now()
    }

    ListView {
        id: toastList

        anchors.fill: parent
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height
        model: NotificationUiState.toastModel
        spacing: Theme.notificationToastColumnGap
        topMargin: Theme.notificationToastColumnTopInset
        bottomMargin: Theme.notificationToastColumnBottomInset

        add: Transition {
            ParallelAnimation {
                NumberAnimation {
                    property: "x"
                    from: toastList.width + Theme.spaceXl
                    duration: Theme.motionSlow
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    property: "opacity"
                    from: 0
                    to: 1
                    duration: Theme.motionNormal
                    easing.type: Easing.OutCubic
                }
            }
        }

        addDisplaced: Transition {
            NumberAnimation {
                properties: "y"
                duration: Theme.motionNormal
                easing.type: Easing.OutCubic
            }
        }

        moveDisplaced: Transition {
            NumberAnimation {
                properties: "y"
                duration: Theme.motionNormal
                easing.type: Easing.OutCubic
            }
        }

        move: Transition {
            NumberAnimation {
                properties: "y"
                duration: Theme.motionNormal
                easing.type: Easing.OutCubic
            }
        }

        remove: Transition {
            ParallelAnimation {
                NumberAnimation {
                    property: "x"
                    to: toastList.width + Theme.spaceLg
                    duration: Theme.motionFast
                    easing.type: Easing.InCubic
                }
                NumberAnimation {
                    property: "opacity"
                    to: 0
                    duration: Theme.motionFast
                    easing.type: Easing.InCubic
                }
            }
        }

        removeDisplaced: Transition {
            NumberAnimation {
                properties: "y"
                duration: Theme.motionNormal
                easing.type: Easing.OutCubic
            }
        }

        delegate: NotificationToastCard {
            required property int notification_id
            required property string app_name
            required property string summary
            required property string body
            required property string icon
            required property string urgency
            required property double timestamp
            required property var actions
            required property int timer_revision

            expanded: NotificationUiState.expandedToastId === notification_id
            notif: ({
                id: notification_id,
                app_name: app_name,
                summary: summary,
                body: body,
                icon: icon,
                urgency: urgency,
                timestamp: timestamp,
                actions: actions || []
            })
            pauseAll: root.pauseAll
            relativeTime: root._relativeTimeLabel(timestamp)
            timerRevision: timer_revision
            width: toastList.width

            onToggleExpandedRequested: notificationId => {
                NotificationUiState.toggleToastExpanded(notificationId);
            }
        }
    }
}
