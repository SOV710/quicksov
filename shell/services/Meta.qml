// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import Quickshell
import "../ipc"

Singleton {
    id: root

    property bool connected: false
    property bool ready: false
    property string lastError: ""
    property string status: "disconnected"

    property string serverVersion: ""
    property int uptimeSec: 0
    property bool configNeedsRestart: false
    property var services: ({})
    property var screenRoles: ({})
    property var powerActions: ({})
    // True only if daemon has published at least one role assignment.
    readonly property bool hasScreenRoles: Object.keys(screenRoles).length > 0

    function _onSnapshot(payload) {
        root.serverVersion       = payload.server_version  || "";
        root.uptimeSec           = payload.uptime_sec      || 0;
        root.configNeedsRestart  = payload.config_needs_restart || false;
        root.services            = payload.services        || {};
        root.screenRoles         = (payload.screens && payload.screens.roles) ? payload.screens.roles : {};
        root.powerActions        = (payload.power && payload.power.actions) ? payload.power.actions : {};
        root.ready   = true;
        root.status  = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("meta", root._onSnapshot);
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
