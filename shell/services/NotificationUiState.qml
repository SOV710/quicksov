// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import QtQml
import Quickshell
import ".."
import "."
import "../ipc"

Singleton {
    id: root

    property bool connected: false
    property int _centerOpenCount: 0
    property int _toastRevisionSeed: 0
    property var _centerVisibility: ({})

    readonly property bool notificationCenterOpen: root._centerOpenCount > 0
    readonly property int toastEnterDurationMs: 24
                                              + DebugVisuals.duration(Theme.motionFast)
                                              + Math.max(
                                                    DebugVisuals.duration(Theme.motionSlow),
                                                    DebugVisuals.duration(Theme.motionNormal)
                                                )
                                              + 48
    readonly property int toastCloseDurationMs: DebugVisuals.duration(Theme.motionFast)
                                              + DebugVisuals.duration(Theme.motionNormal)
                                              + 48
    readonly property bool toastSurfaceActive: toastModel.count > 0
    readonly property alias toastModel: toastModel

    function _nextToastRevision() {
        root._toastRevisionSeed += 1;
        return root._toastRevisionSeed;
    }

    function _nextToastLifecycleDelayMs(nowMs) {
        var nextDelay = -1;
        var now = nowMs !== undefined ? nowMs : Date.now();

        for (var i = 0; i < toastModel.count; ++i) {
            var entry = toastModel.get(i);
            var dueAt = 0;

            if (entry.lifecycle_state === "entering")
                dueAt = entry.enter_complete_at_ms || 0;
            else if (entry.lifecycle_state === "closing")
                dueAt = entry.close_complete_at_ms || 0;

            if (dueAt <= 0)
                continue;

            var delay = Math.max(0, Math.ceil(dueAt - now));
            if (nextDelay < 0 || delay < nextDelay)
                nextDelay = delay;
        }

        return nextDelay;
    }

    function _scheduleToastLifecycleSweep(nowMs) {
        var nextDelay = root._nextToastLifecycleDelayMs(nowMs);
        if (nextDelay < 0) {
            toastLifecycleTimer.stop();
            return;
        }

        toastLifecycleTimer.interval = nextDelay;
        toastLifecycleTimer.restart();
    }

    function _advanceToastLifecycle(nowMs) {
        var now = nowMs !== undefined ? nowMs : Date.now();

        for (var i = toastModel.count - 1; i >= 0; --i) {
            var entry = toastModel.get(i);

            if (entry.lifecycle_state === "closing"
                    && (entry.close_complete_at_ms || 0) > 0
                    && entry.close_complete_at_ms <= now) {
                toastModel.remove(i);
                continue;
            }

            if (entry.lifecycle_state === "entering"
                    && (entry.enter_complete_at_ms || 0) > 0
                    && entry.enter_complete_at_ms <= now) {
                entry.lifecycle_state = "open";
                entry.enter_complete_at_ms = 0;
                entry.close_complete_at_ms = 0;
                toastModel.set(i, entry);
            }
        }

        root._scheduleToastLifecycleSweep(now);
    }

    function _toastEntry(notification, lifecycleState, lifecycleRevision, nowMs) {
        var now = nowMs !== undefined ? nowMs : Date.now();
        return {
            notification_id: notification.id,
            app_name: notification.app_name || "",
            summary: notification.summary || "",
            body: notification.body || "",
            icon: notification.icon || "",
            urgency: notification.urgency || "normal",
            timestamp: notification.timestamp || Date.now(),
            actions: notification.actions || [],
            timer_revision: root._nextToastRevision(),
            lifecycle_state: lifecycleState || "open",
            lifecycle_revision: lifecycleRevision !== undefined
                                ? lifecycleRevision
                                : root._nextToastRevision(),
            enter_complete_at_ms: lifecycleState === "entering"
                                  ? now + root.toastEnterDurationMs
                                  : 0,
            close_complete_at_ms: lifecycleState === "closing"
                                  ? now + root.toastCloseDurationMs
                                  : 0
        };
    }

    function _toastIndex(notificationId) {
        for (var i = 0; i < toastModel.count; ++i) {
            if (toastModel.get(i).notification_id === notificationId)
                return i;
        }

        return -1;
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribeEvents("notification", root._onEvent);
        } else {
            root.clearToastState();
        }
    }

    function _onEvent(eventName, payload) {
        switch (eventName) {
        case "new":
            root.upsertToast(payload);
            break;
        case "closed":
            if (payload && payload.id !== undefined)
                root.beginToastClose(payload.id);
            break;
        default:
            break;
        }
    }

    function clearToastState() {
        toastModel.clear();
        toastLifecycleTimer.stop();
    }

    function beginToastClose(notificationId) {
        var index = root._toastIndex(notificationId);
        if (index < 0)
            return false;

        var entry = toastModel.get(index);
        if (entry.lifecycle_state === "closing") {
            if ((entry.close_complete_at_ms || 0) <= 0) {
                entry.close_complete_at_ms = Date.now() + root.toastCloseDurationMs;
                toastModel.set(index, entry);
                root._scheduleToastLifecycleSweep();
            }
            return false;
        }

        entry.lifecycle_state = "closing";
        entry.lifecycle_revision = root._nextToastRevision();
        entry.enter_complete_at_ms = 0;
        entry.close_complete_at_ms = Date.now() + root.toastCloseDurationMs;
        toastModel.set(index, entry);
        root._scheduleToastLifecycleSweep();
        return true;
    }

    function setNotificationCenterVisible(key, visible) {
        var nextVisibility = Object.assign({}, root._centerVisibility);
        if (visible)
            nextVisibility[key] = true;
        else
            delete nextVisibility[key];

        var nextCount = Object.keys(nextVisibility).length;
        var wasClosed = root._centerOpenCount === 0;

        root._centerVisibility = nextVisibility;
        root._centerOpenCount = nextCount;

        if (wasClosed && nextCount > 0)
            root.clearToastState();
    }

    function dismissToastPreview(notificationId) {
        root.beginToastClose(notificationId);
    }

    function invokeToastAction(notificationId, actionId) {
        if (notificationId < 0 || !actionId || root.notificationCenterOpen)
            return;

        root.beginToastClose(notificationId);
        Notification.invokeActionAndDismiss(notificationId, actionId);
    }

    function upsertToast(notification) {
        if (!notification || notification.id === undefined || root.notificationCenterOpen)
            return;

        var now = Date.now();
        var index = root._toastIndex(notification.id);
        var lifecycleState = "entering";
        var lifecycleRevision = root._nextToastRevision();

        if (index >= 0) {
            var current = toastModel.get(index);
            if (current.lifecycle_state === "entering") {
                lifecycleState = "entering";
                lifecycleRevision = current.lifecycle_revision;
            } else if (current.lifecycle_state === "closing") {
                lifecycleState = "entering";
            } else {
                lifecycleState = "open";
                lifecycleRevision = current.lifecycle_revision;
            }
        }

        var entry = root._toastEntry(notification, lifecycleState, lifecycleRevision, now);
        if (index === 0) {
            toastModel.set(0, entry);
        } else if (index > 0) {
            toastModel.move(index, 0, 1);
            toastModel.set(0, entry);
        } else {
            toastModel.insert(0, entry);
        }

        root._scheduleToastLifecycleSweep(now);
    }

    ListModel {
        id: toastModel
        dynamicRoles: true
    }

    Timer {
        id: toastLifecycleTimer

        interval: 0
        repeat: false
        running: false
        onTriggered: root._advanceToastLifecycle(Date.now())
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected) root._onConnectionChanged(true);
    }
}
