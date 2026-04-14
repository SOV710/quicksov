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
    property string wifiState: ""

    function _onLinkSnapshot(payload) {
        root.interfaces = payload.interfaces || [];
        root.ready  = true;
        root.status = "ok";
    }

    function _onWifiSnapshot(payload) {
        root.wifiAvailable = payload.available || false;
        root.wifiConnected = payload.connected || false;
        root.ssid          = payload.ssid      || "";
        root.signalDbm     = payload.signal_dbm || 0;
        root.wifiState     = payload.state      || "";
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
