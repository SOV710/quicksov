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
    property bool ready: false
    property string lastError: ""
    property string status: "disconnected"

    property int count: 0
    property bool hasUnread: false
    property var notifications: []
    property alias notificationModel: notificationModel
    property var _actionsById: ({})

    function _findNotificationRow(notificationId) {
        for (var i = 0; i < notificationModel.count; ++i) {
            if (notificationModel.get(i).notification_id === notificationId)
                return i;
        }
        return -1;
    }

    function _notificationRoleMap(notification) {
        return {
            notification_id: notification && notification.id !== undefined ? notification.id : -1,
            app_name: notification && notification.app_name ? notification.app_name : "",
            summary: notification && notification.summary ? notification.summary : "",
            body: notification && notification.body ? notification.body : "",
            icon: notification && notification.icon ? notification.icon : "",
            urgency: notification && notification.urgency ? notification.urgency : "normal",
            timestamp: notification && notification.timestamp !== undefined ? notification.timestamp : 0
        };
    }

    function _setNotificationRow(row, mapped) {
        for (var key in mapped) {
            if (notificationModel.get(row)[key] !== mapped[key])
                notificationModel.setProperty(row, key, mapped[key]);
        }
    }

    function _syncNotificationModel(nextNotifications) {
        var items = nextNotifications || [];
        var nextActionsById = ({});

        for (var i = 0; i < items.length; ++i) {
            var notification = items[i];
            if (!notification || notification.id === undefined)
                continue;
            nextActionsById[notification.id] = notification.actions || [];
        }

        for (var row = notificationModel.count - 1; row >= 0; --row) {
            var currentId = notificationModel.get(row).notification_id;
            var keep = false;

            for (var idx = 0; idx < items.length; ++idx) {
                if (items[idx] && items[idx].id === currentId) {
                    keep = true;
                    break;
                }
            }

            if (!keep)
                notificationModel.remove(row);
        }

        for (var targetRow = 0; targetRow < items.length; ++targetRow) {
            var mapped = root._notificationRoleMap(items[targetRow]);
            var existingRow = root._findNotificationRow(mapped.notification_id);

            if (existingRow < 0) {
                if (targetRow >= notificationModel.count)
                    notificationModel.append(mapped);
                else
                    notificationModel.insert(targetRow, mapped);
                continue;
            }

            if (existingRow !== targetRow)
                notificationModel.move(existingRow, targetRow, 1);

            root._setNotificationRow(targetRow, mapped);
        }

        root._actionsById = nextActionsById;
    }

    function actionsFor(notificationId) {
        return root._actionsById[notificationId] || [];
    }

    function hasNotification(notificationId) {
        return root._findNotificationRow(notificationId) >= 0;
    }

    function dismiss(id) {
        Client.request("notification", "dismiss", { id: id }, null);
    }
    function dismissAll() {
        Client.request("notification", "dismiss_all", {}, null);
    }
    function markRead(id) {
        var payload = {};
        if (id !== undefined && id !== null)
            payload.id = id;
        Client.request("notification", "mark_read", payload, null);
    }
    function invokeAction(id, actionKey) {
        Client.request("notification", "invoke_action", { id: id, action_id: actionKey }, null);
    }
    function invokeActionAndDismiss(id, actionKey) {
        Client.request("notification", "invoke_action_and_dismiss", { id: id, action_id: actionKey }, null);
    }

    function _onSnapshot(payload) {
        var unread = payload.unread_count || 0;
        var history = payload.history || [];
        root.count         = unread;
        root.hasUnread     = unread > 0;
        root.notifications = history;
        root._syncNotificationModel(history);
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("notification", root._onSnapshot);
        } else {
            root.count = 0;
            root.hasUnread = false;
            root.notifications = [];
            root._actionsById = ({});
            notificationModel.clear();
            root.ready  = false;
            root.status = "disconnected";
        }
    }

    ListModel {
        id: notificationModel
        dynamicRoles: true
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected) root._onConnectionChanged(true);
    }
}
