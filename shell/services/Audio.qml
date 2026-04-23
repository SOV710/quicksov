// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import QtQml
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
    property var streams: []
    property alias streamsModel: streamsModel

    // Derived from resolved sink object; daemon uses volume_pct (0–100 int → normalise to 0–1)
    property real volume: defaultSink ? (defaultSink.volume_pct / 100.0) : 0.0
    property bool muted:  defaultSink ? (defaultSink.muted === true)     : false

    function setVolume(sinkId, vol) {
        var clamped = Math.max(0.0, Math.min(1.5, vol));
        Client.request("audio", "set_volume", { sink_id: sinkId, volume_pct: Math.round(clamped * 100) }, null);
    }

    function setStreamVolume(streamId, vol) {
        var clamped = Math.max(0.0, Math.min(1.5, vol));
        Client.request("audio", "set_stream_volume", { stream_id: streamId, volume_pct: Math.round(clamped * 100) }, null);
    }

    function setMuted(sinkId, muted) {
        Client.request("audio", "set_mute", { sink_id: sinkId, muted: muted }, null);
    }

    function setDefaultSink(sinkId) {
        Client.request("audio", "set_default_sink", { sink_id: sinkId }, null);
    }

    function _streamRoleMap(stream) {
        return {
            stream_id: stream && stream.id !== undefined ? stream.id : -1,
            app_name: stream && stream.app_name ? stream.app_name : "",
            binary: stream && stream.binary ? stream.binary : "",
            title: stream && stream.title ? stream.title : "",
            icon: stream && stream.icon ? stream.icon : "",
            volume_pct: stream && stream.volume_pct !== undefined ? stream.volume_pct : 0,
            muted: stream && stream.muted === true
        };
    }

    function _findStreamRow(streamId) {
        for (var i = 0; i < streamsModel.count; ++i) {
            if (streamsModel.get(i).stream_id === streamId) return i;
        }
        return -1;
    }

    function _setStreamRow(row, mapped) {
        for (var key in mapped) {
            if (streamsModel.get(row)[key] !== mapped[key]) {
                streamsModel.setProperty(row, key, mapped[key]);
            }
        }
    }

    function _syncStreamsModel(nextStreams) {
        var items = nextStreams || [];

        for (var row = streamsModel.count - 1; row >= 0; --row) {
            var currentId = streamsModel.get(row).stream_id;
            var keep = false;
            for (var i = 0; i < items.length; ++i) {
                if (items[i] && items[i].id === currentId) {
                    keep = true;
                    break;
                }
            }
            if (!keep) streamsModel.remove(row);
        }

        for (var idx = 0; idx < items.length; ++idx) {
            var mapped = root._streamRoleMap(items[idx]);
            var existingRow = root._findStreamRow(mapped.stream_id);

            if (existingRow < 0) {
                if (idx >= streamsModel.count) streamsModel.append(mapped);
                else streamsModel.insert(idx, mapped);
                continue;
            }

            if (existingRow !== idx) {
                streamsModel.move(existingRow, idx, 1);
            }

            root._setStreamRow(idx, mapped);
        }
    }

    function _onSnapshot(payload) {
        root.sinks   = payload.sinks   || [];
        root.sources = payload.sources || [];
        root.streams = payload.streams || [];
        root._syncStreamsModel(root.streams);

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
            root.defaultSink = null;
            root.defaultSource = null;
            root.sinks = [];
            root.sources = [];
            root.streams = [];
            streamsModel.clear();
            root.ready  = false;
            root.status = "disconnected";
        }
    }

    ListModel {
        id: streamsModel
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected) root._onConnectionChanged(true);
    }
}
