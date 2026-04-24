// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import QtQml
import Quickshell
import "../ipc"

Singleton {
    id: root

    property bool connected: false
    property int expandedToastId: -1
    property int _centerOpenCount: 0
    property int _toastRevisionSeed: 0
    property var _centerVisibility: ({})

    readonly property bool notificationCenterOpen: root._centerOpenCount > 0
    readonly property alias toastModel: toastModel

    function _toastEntry(notification) {
        root._toastRevisionSeed += 1;
        return {
            notification_id: notification.id,
            app_name: notification.app_name || "",
            summary: notification.summary || "",
            body: notification.body || "",
            icon: notification.icon || "",
            urgency: notification.urgency || "normal",
            timestamp: notification.timestamp || Date.now(),
            actions: notification.actions || [],
            timer_revision: root._toastRevisionSeed
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
                root.dismissToastPreview(payload.id);
            break;
        default:
            break;
        }
    }

    function clearToastState() {
        toastModel.clear();
        root.expandedToastId = -1;
    }

    function dismissToastPreview(notificationId) {
        var index = root._toastIndex(notificationId);
        if (index >= 0)
            toastModel.remove(index);
        if (root.expandedToastId === notificationId)
            root.expandedToastId = -1;
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

    function toggleToastExpanded(notificationId) {
        root.expandedToastId = root.expandedToastId === notificationId ? -1 : notificationId;
    }

    function upsertToast(notification) {
        if (!notification || notification.id === undefined || root.notificationCenterOpen)
            return;

        var entry = root._toastEntry(notification);
        var index = root._toastIndex(entry.notification_id);
        if (index === 0) {
            toastModel.set(0, entry);
        } else if (index > 0) {
            toastModel.move(index, 0, 1);
            toastModel.set(0, entry);
        } else {
            toastModel.insert(0, entry);
        }

        if (root.expandedToastId !== entry.notification_id && root.expandedToastId >= 0) {
            var expandedIndex = root._toastIndex(root.expandedToastId);
            if (expandedIndex < 0)
                root.expandedToastId = -1;
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
