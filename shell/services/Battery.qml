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

    property bool present: false
    property string chargeStatus: ""
    property real percentage: 0.0
    property var timeToEmptySec: null
    property var timeToFullSec: null
    property string technology: ""

    function _onSnapshot(payload) {
        root.present        = payload.present        || false;
        root.chargeStatus   = payload.status         || "";
        root.percentage     = payload.percentage     || 0.0;
        root.timeToEmptySec = payload.time_to_empty_sec;
        root.timeToFullSec  = payload.time_to_full_sec;
        root.technology     = payload.technology     || "";
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("battery", root._onSnapshot);
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
