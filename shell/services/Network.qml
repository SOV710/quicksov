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

    // net.link
    property var interfaces: []

    // net.wifi
    property bool wifiAvailable: false
    property bool wifiConnected: false
    property string ssid: ""
    property int signalDbm: 0
    property int signalPct: -1
    property string wifiState: ""

    function _onLinkSnapshot(payload) {
        root.interfaces = payload.interfaces || [];
        root.ready  = true;
        root.status = "ok";
    }

    // net.wifi — daemon sends state string, not a bool "connected"
    function _onWifiSnapshot(payload) {
        root.wifiState     = payload.state    || "";
        // state is mapped by daemon: "connected" | "scanning" | "associating" | "disconnected" | "unknown"
        root.wifiConnected = root.wifiState === "connected";
        root.wifiAvailable = root.wifiState !== "unknown";
        root.ssid          = payload.ssid     || "";
        // Daemon field is rssi_dbm, not signal_dbm
        root.signalDbm     = payload.rssi_dbm || 0;
        root.signalPct     = typeof payload.signal_pct === "number" ? payload.signal_pct : -1;
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("net.link", root._onLinkSnapshot);
            Client.subscribe("net.wifi", root._onWifiSnapshot);
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
