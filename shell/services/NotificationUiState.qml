// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import QtQml
import Quickshell
import "."
import "../ipc"

Singleton {
    id: root

    property bool connected: false
    property int _centerOpenCount: 0
    property int _toastRevisionSeed: 0
    property var _centerVisibility: ({})

    readonly property bool notificationCenterOpen: root._centerOpenCount > 0
    readonly property bool toastSurfaceActive: toastModel.count > 0
    readonly property alias toastModel: toastModel

    function _nextToastRevision() {
        root._toastRevisionSeed += 1;
        return root._toastRevisionSeed;
    }

    function _toastEntry(notification, lifecycleState, lifecycleRevision) {
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
                                : root._nextToastRevision()
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
    }

    function beginToastClose(notificationId) {
        var index = root._toastIndex(notificationId);
        if (index < 0)
            return false;

        var entry = toastModel.get(index);
        if (entry.lifecycle_state === "closing")
            return false;

        entry.lifecycle_state = "closing";
        entry.lifecycle_revision = root._nextToastRevision();
        toastModel.set(index, entry);
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

    function finalizeToastRemoval(notificationId, lifecycleRevision) {
        var index = root._toastIndex(notificationId);
        if (index < 0)
            return;

        var entry = toastModel.get(index);
        if (entry.lifecycle_state !== "closing" || entry.lifecycle_revision !== lifecycleRevision)
            return;

        toastModel.remove(index);
    }

    function invokeToastAction(notificationId, actionId) {
        if (notificationId < 0 || !actionId || root.notificationCenterOpen)
            return;

        root.beginToastClose(notificationId);
        Notification.invokeActionAndDismiss(notificationId, actionId);
    }

    function markToastEntered(notificationId, lifecycleRevision) {
        var index = root._toastIndex(notificationId);
        if (index < 0)
            return;

        var entry = toastModel.get(index);
        if (entry.lifecycle_state !== "entering" || entry.lifecycle_revision !== lifecycleRevision)
            return;

        entry.lifecycle_state = "open";
        toastModel.set(index, entry);
    }

    function upsertToast(notification) {
        if (!notification || notification.id === undefined || root.notificationCenterOpen)
            return;

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

        var entry = root._toastEntry(notification, lifecycleState, lifecycleRevision);
        if (index === 0) {
            toastModel.set(0, entry);
        } else if (index > 0) {
            toastModel.move(index, 0, 1);
            toastModel.set(0, entry);
        } else {
            toastModel.insert(0, entry);
        }
    }

    ListModel {
        id: toastModel
        dynamicRoles: true
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected) root._onConnectionChanged(true);
    }
}
