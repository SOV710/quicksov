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

    property var players: []
    property string activeBusName: ""
    property var activePlayer: {
        for (var i = 0; i < players.length; ++i) {
            if (players[i].bus_name === activeBusName)
                return players[i];
        }
        return players.length > 0 ? players[0] : null;
    }

    function playPause(busName) {
        Client.request("mpris", "play_pause", { bus_name: busName }, null);
    }
    function next(busName) {
        Client.request("mpris", "next", { bus_name: busName }, null);
    }
    function previous(busName) {
        Client.request("mpris", "prev", { bus_name: busName }, null);
    }

    function _onSnapshot(payload) {
        root.players = payload.players || [];
        root.activeBusName = payload.active_player || "";
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("mpris", root._onSnapshot);
        } else {
            root.players = [];
            root.activeBusName = "";
            root.ready  = false;
            root.status = "disconnected";
        }
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected) root._onConnectionChanged(true);
    }
}
