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

    property bool btAvailable: false
    property bool btEnabled: false
    property bool discovering: false
    property var devices: []

    property var connectedDevices: {
        return root.devices.filter(function(d) { return d.connected; });
    }

    function _onSnapshot(payload) {
        root.btAvailable = payload.available   || false;
        root.btEnabled   = payload.enabled     || false;
        root.discovering = payload.discovering || false;
        root.devices     = payload.devices     || [];
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("bluetooth", root._onSnapshot);
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
