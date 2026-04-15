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

    property var defaultSink: null
    property var defaultSource: null
    property var sinks: []
    property var sources: []

    property real volume: (defaultSink && defaultSink.volume != null) ? defaultSink.volume : 0.0
    property bool muted:  (defaultSink && defaultSink.muted  != null) ? defaultSink.muted  : false

    function setVolume(sinkName, vol) {
        Client.request("audio", "set_volume", { sink: sinkName, volume: vol }, null);
    }

    function setMuted(sinkName, muted) {
        Client.request("audio", "set_muted", { sink: sinkName, muted: muted }, null);
    }

    function setDefaultSink(sinkName) {
        Client.request("audio", "set_default_sink", { sink: sinkName }, null);
    }

    function _onSnapshot(payload) {
        root.defaultSink   = payload.default_sink   || null;
        root.defaultSource = payload.default_source || null;
        root.sinks         = payload.sinks          || [];
        root.sources       = payload.sources        || [];
        root.ready  = true;
        root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("audio", root._onSnapshot);
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
