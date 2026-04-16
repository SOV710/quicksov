// SPDX-FileCopyrightText: 2026 SOV710
//
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

    property var workspaces: []
    property var windows: []
    property var focusedWindow: null
    property var outputs: []

    function focusWorkspace(index) {
        // Daemon expects { idx: N }, not { reference: { Index: N } }
        Client.request("niri", "focus_workspace", { idx: index }, null);
    }
    function moveColumnToWorkspace(index) {
        Client.request("niri", "move_column_to_workspace", { reference: { Index: index } }, null);
    }
    function niriAction(action) {
        Client.request("niri", "niri_action", action, null);
    }

    function workspacesForOutput(outputName) {
        return root.workspaces.filter(function(ws) { return ws.output === outputName; });
    }

    function _onSnapshot(payload) {
        root.workspaces    = payload.workspaces    || [];
        root.windows       = payload.windows       || [];
        root.focusedWindow = payload.focused_window || null;
        root.outputs       = payload.outputs       || [];
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("niri", root._onSnapshot);
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
