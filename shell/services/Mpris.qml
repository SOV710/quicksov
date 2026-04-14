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

    property var players: []
    property var activePlayer: players.length > 0 ? players[0] : null

    function playPause(busName) {
        Client.request("mpris", "play_pause", { bus_name: busName }, null);
    }
    function next(busName) {
        Client.request("mpris", "next", { bus_name: busName }, null);
    }
    function previous(busName) {
        Client.request("mpris", "previous", { bus_name: busName }, null);
    }
    function setVolume(busName, vol) {
        Client.request("mpris", "set_volume", { bus_name: busName, volume: vol }, null);
    }

    function _onSnapshot(payload) {
        root.players = payload.players || [];
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("mpris", root._onSnapshot);
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
