// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import Quickshell
import "../ipc"
import "../ipc/protocol.js" as Protocol

Singleton {
    id: root

    property bool connected: false
    property bool ready: false
    property string lastError: ""
    property string status: "disconnected"

    property string directory: ""
    property string availability: "unavailable"
    property string availabilityReason: "none"
    property var entries: []
    property string fallbackSource: ""
    property var sources: ({})
    property var views: ({})
    property string transitionType: "fade"
    property int transitionDurationMs: 320

    property string rendererProcess: "qsov-wallpaperd"
    property string rendererBackend: "native-wayland-ffmpeg"
    property string rendererStatus: "starting"
    property int rendererPid: 0
    property string rendererLastError: ""
    property var decodeBackendOrder: []
    property string presentMode: ""
    property bool vsync: true
    property bool videoAudio: false

    readonly property bool hasSources: Object.keys(root.sources || {}).length > 0

    function _resetState() {
        root.directory = "";
        root.availability = "unavailable";
        root.availabilityReason = "none";
        root.entries = [];
        root.fallbackSource = "";
        root.sources = ({});
        root.views = ({});
        root.transitionType = "fade";
        root.transitionDurationMs = 320;
        root.rendererProcess = "qsov-wallpaperd";
        root.rendererBackend = "native-wayland-ffmpeg";
        root.rendererStatus = "starting";
        root.rendererPid = 0;
        root.rendererLastError = "";
        root.decodeBackendOrder = [];
        root.presentMode = "";
        root.vsync = true;
        root.videoAudio = false;
        root.lastError = "";
    }

    function _request(action, payload) {
        Client.request("wallpaper", action, payload || {}, function(msg) {
            if (!msg)
                return;
            if (msg.kind === Protocol.Kind.ERR) {
                var body = msg.payload || {};
                root.lastError = body.message || body.code || "Wallpaper request failed";
                root.status = "error";
                errorClearTimer.restart();
            } else if (root.connected) {
                root.lastError = "";
                if (root.rendererStatus === "error")
                    root.status = "degraded";
                else
                    root.status = "ok";
            }
        });
    }

    function refresh() {
        root._request("refresh", {});
    }

    function setOutputSource(output, source) {
        if (!output || !source)
            return;
        root._request("set_output_source", { output: String(output), source: String(source) });
    }

    function setOutputPath(output, path) {
        if (!output || !path)
            return;
        root._request("set_output_path", { output: String(output), path: String(path) });
    }

    function nextOutput(output) {
        if (!output)
            return;
        root._request("next_output", { output: String(output) });
    }

    function prevOutput(output) {
        if (!output)
            return;
        root._request("prev_output", { output: String(output) });
    }

    function setOutputCrop(output, crop) {
        if (!output)
            return;
        root._request("set_output_crop", { output: String(output), crop: crop === undefined ? null : crop });
    }

    function sourceById(sourceId) {
        if (!sourceId || !root.sources)
            return null;
        return root.sources[sourceId] || null;
    }

    function viewForOutput(outputName) {
        if (!outputName || !root.views)
            return null;
        return root.views[outputName] || null;
    }

    function sourceIdForOutput(outputName) {
        var view = root.viewForOutput(outputName);
        if (view && typeof view.source === "string" && view.source.length > 0)
            return view.source;
        return root.fallbackSource || "";
    }

    function sourceForOutput(outputName) {
        var sourceId = root.sourceIdForOutput(outputName);
        return root.sourceById(sourceId);
    }

    function cropForOutput(outputName) {
        var view = root.viewForOutput(outputName);
        return view && view.crop ? view.crop : null;
    }

    function isVideoForOutput(outputName) {
        var source = root.sourceForOutput(outputName);
        return source && source.kind === "video";
    }

    function isImageForOutput(outputName) {
        var source = root.sourceForOutput(outputName);
        return source && source.kind === "image";
    }

    function _onSnapshot(payload) {
        var transition = payload.transition || {};
        var renderer = payload.renderer || {};

        root.directory = payload.directory || "";
        root.availability = payload.availability || "unavailable";
        root.availabilityReason = payload.availability_reason || "none";
        root.entries = payload.entries || [];
        root.fallbackSource = payload.fallback_source || "";
        root.sources = payload.sources || ({});
        root.views = payload.views || ({});
        root.transitionType = transition.type || "fade";
        root.transitionDurationMs = typeof transition.duration_ms === "number"
            ? transition.duration_ms
            : 320;
        root.rendererProcess = renderer.process || "qsov-wallpaperd";
        root.rendererBackend = renderer.backend || "native-wayland-ffmpeg";
        root.rendererStatus = renderer.status || "starting";
        root.rendererPid = typeof renderer.pid === "number" ? renderer.pid : 0;
        root.rendererLastError = renderer.last_error || "";
        root.decodeBackendOrder = renderer.decode_backend_order || [];
        root.presentMode = renderer.present_mode || "";
        root.vsync = typeof renderer.vsync === "boolean" ? renderer.vsync : true;
        root.videoAudio = typeof renderer.video_audio === "boolean" ? renderer.video_audio : false;
        root.ready = true;
        root.lastError = "";

        if (root.rendererStatus === "error")
            root.status = "degraded";
        else
            root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("wallpaper", root._onSnapshot);
        } else {
            root._resetState();
            root.ready = false;
            root.status = "disconnected";
        }
    }

    Timer {
        id: errorClearTimer
        interval: 6000
        repeat: false
        onTriggered: {
            root.lastError = "";
            if (root.connected)
                root.status = root.rendererStatus === "error" ? "degraded" : "ok";
        }
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected)
            root._onConnectionChanged(true);
    }
}
