// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property double nowMs: Date.now()

    readonly property bool pauseAll: columnHover.hovered || toastFlick.dragging

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

    Flickable {
        id: toastFlick
        anchors.fill: parent
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        contentWidth: width
        contentHeight: toastContent.implicitHeight
        flickableDirection: Flickable.VerticalFlick
        interactive: contentHeight > height

        Item {
            id: toastContent

            width: toastFlick.width
            implicitHeight: Theme.notificationToastColumnTopInset
                            + toastColumn.height
                            + Theme.notificationToastColumnBottomInset

            Column {
                id: toastColumn

                y: Theme.notificationToastColumnTopInset
                width: parent.width
                height: childrenRect.height
                spacing: Theme.notificationToastColumnGap

                Repeater {
                    model: NotificationUiState.toastModel

                    delegate: Item {
                        property int notificationId: notification_id
                        property string appName: app_name
                        property string summaryText: summary
                        property string bodyText: body
                        property string iconPath: icon
                        property string urgencyLevel: urgency
                        property double notificationTimestamp: timestamp
                        property var notificationActions: actions
                        property int timerRevisionValue: timer_revision
                        property string lifecycleStateValue: lifecycle_state
                        property int lifecycleRevisionValue: lifecycle_revision

                        function syncCardLifecycle() {
                            toastCard.toastLifecycleRevision = lifecycleRevisionValue;
                            toastCard.toastLifecycleState = lifecycleStateValue;
                        }

                        width: toastColumn.width
                        implicitHeight: toastCard.height
                        height: toastCard.height

                        Component.onCompleted: syncCardLifecycle()
                        onLifecycleRevisionValueChanged: syncCardLifecycle()
                        onLifecycleStateValueChanged: syncCardLifecycle()

                        NotificationToastCard {
                            id: toastCard

                            notif: ({
                                id: parent.notificationId,
                                app_name: parent.appName,
                                summary: parent.summaryText,
                                body: parent.bodyText,
                                icon: parent.iconPath,
                                urgency: parent.urgencyLevel,
                                timestamp: parent.notificationTimestamp,
                                actions: parent.notificationActions || []
                            })
                            pauseAll: root.pauseAll
                            relativeTime: root._relativeTimeLabel(parent.notificationTimestamp)
                            timerRevision: parent.timerRevisionValue
                            width: parent.width
                        }
                    }
                }
            }
        }
    }
}
