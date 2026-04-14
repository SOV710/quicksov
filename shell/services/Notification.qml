// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import "../ipc"

QtObject {
    id: root

    property bool connected: false
    property bool ready: false
    property string lastError: ""
    property string status: "disconnected"

    property int count: 0
    property bool hasUnread: false
    property var notifications: []

    function dismiss(id) {
        Client.request("notification", "dismiss", { id: id }, null);
    }
    function dismissAll() {
        Client.request("notification", "dismiss_all", {}, null);
    }
    function invokeAction(id, actionKey) {
        Client.request("notification", "invoke_action", { id: id, action_key: actionKey }, null);
    }

    function _onSnapshot(payload) {
        root.count         = payload.count          || 0;
        root.hasUnread     = payload.has_unread      || false;
        root.notifications = payload.notifications   || [];
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("notification", root._onSnapshot);
        } else {
            root.ready  = false;
            root.status = "disconnected";
        }
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected) root._onConnectionChanged(true);
    }
}
