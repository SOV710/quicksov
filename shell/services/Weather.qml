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

    property bool available: false
    property string location: ""
    property var current: null
    property var hourlyForecast: []
    property var lastUpdatedMs: null
    property string weatherError: ""

    function refresh() {
        Client.request("weather", "refresh", {}, null);
    }

    function _onSnapshot(payload) {
        root.available      = payload.available      || false;
        root.location       = payload.location       || "";
        root.current        = payload.current        || null;
        root.hourlyForecast = payload.hourly_forecast || [];
        root.lastUpdatedMs  = payload.last_updated_ms;
        root.weatherError   = payload.error          || "";
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("weather", root._onSnapshot);
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
