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

    property bool available: false
    property var location: null
    property var current: null
    property var hourlyForecast: []
    property var lastUpdatedMs: null
    property string weatherError: ""

    function refresh() {
        Client.request("weather", "refresh", {}, null);
    }

    function _onSnapshot(payload) {
        root.available      = payload.offline !== true;
        root.location       = payload.location || null;
        root.current        = payload.current || null;
        root.hourlyForecast = payload.hourly || [];
        root.lastUpdatedMs  = payload.updated_at !== undefined ? payload.updated_at * 1000 : null;
        root.weatherError   = payload.offline === true ? "offline" : "";
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("weather", root._onSnapshot);
        } else {
            root.available = false;
            root.location = null;
            root.current = null;
            root.hourlyForecast = [];
            root.lastUpdatedMs = null;
            root.weatherError = "";
            root.ready  = false;
            root.status = "disconnected";
        }
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected) root._onConnectionChanged(true);
    }
}
