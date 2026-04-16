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

    property bool available: root.lastSuccessMs !== null
    property string provider: ""
    property string fetchStatus: "loading"
    property int ttlSec: 0
    property var location: null
    property var current: null
    property var hourlyForecast: []
    property var lastSuccessMs: null
    property var errorInfo: null
    property string weatherError: ""

    function refresh() {
        Client.request("weather", "refresh", {}, null);
    }

    function _onSnapshot(payload) {
        root.provider       = payload.provider || "";
        root.fetchStatus    = payload.status || "loading";
        root.ttlSec         = typeof payload.ttl_sec === "number" ? payload.ttl_sec : 0;
        root.location       = payload.location || null;
        root.current        = payload.current || null;
        root.hourlyForecast = payload.hourly || [];
        root.lastSuccessMs  = payload.last_success_at !== undefined && payload.last_success_at !== null
            ? payload.last_success_at * 1000
            : null;
        root.errorInfo      = payload.error || null;
        root.weatherError   = root.errorInfo && root.errorInfo.kind ? root.errorInfo.kind : "";
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("weather", root._onSnapshot);
        } else {
            root.provider = "";
            root.fetchStatus = "loading";
            root.ttlSec = 0;
            root.location = null;
            root.current = null;
            root.hourlyForecast = [];
            root.lastSuccessMs = null;
            root.errorInfo = null;
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
