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

    property var defaultSink: null
    property var defaultSource: null
    property var sinks: []
    property var sources: []

    // Derived from resolved sink object; daemon uses volume_pct (0–100 int → normalise to 0–1)
    property real volume: defaultSink ? (defaultSink.volume_pct / 100.0) : 0.0
    property bool muted:  defaultSink ? (defaultSink.muted === true)     : false

    function setVolume(sinkName, vol) {
        // Daemon expects volume_pct (0–100 integer)
        Client.request("audio", "set_volume", { sink: sinkName, volume_pct: Math.round(vol * 100) }, null);
    }

    function setMuted(sinkName, muted) {
        // Daemon action is "set_mute" not "set_muted"
        Client.request("audio", "set_mute", { sink: sinkName, muted: muted }, null);
    }

    function setDefaultSink(sinkName) {
        Client.request("audio", "set_default_sink", { sink: sinkName }, null);
    }

    function _onSnapshot(payload) {
        root.sinks   = payload.sinks   || [];
        root.sources = payload.sources || [];

        // default_sink is a NAME string; resolve to the sink object in sinks[]
        var dsName = payload.default_sink || "";
        root.defaultSink = root.sinks.find(function(s) { return s.name === dsName; }) || null;

        var srcName = payload.default_source || "";
        root.defaultSource = root.sources.find(function(s) { return s.name === srcName; }) || null;

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
