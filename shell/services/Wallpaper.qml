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
    property var current: null
    property string transitionType: "fade"
    property int transitionDurationMs: 320
    property string renderBackend: "mpv"
    property bool videoEnabled: true
    property bool videoAudio: false

    readonly property string currentPath: current && typeof current.path === "string"
                                        ? current.path
                                        : ""
    readonly property string currentKind: current && typeof current.kind === "string"
                                        ? current.kind
                                        : ""
    readonly property bool hasCurrentEntry: availability === "ready" && currentPath !== ""
    readonly property bool hasRenderableImage: hasCurrentEntry && currentKind === "image"
    readonly property bool hasRenderableVideo: hasCurrentEntry
                                            && currentKind === "video"
                                            && videoEnabled
                                            && renderBackend === "mpv"

    function _resetState() {
        root.directory = "";
        root.availability = "unavailable";
        root.availabilityReason = "none";
        root.entries = [];
        root.current = null;
        root.transitionType = "fade";
        root.transitionDurationMs = 320;
        root.renderBackend = "mpv";
        root.videoEnabled = true;
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
                root.status = "ok";
            }
        });
    }

    function refresh() {
        root._request("refresh", {});
    }

    function next() {
        root._request("next", {});
    }

    function prev() {
        root._request("prev", {});
    }

    function setPath(path) {
        if (!path || String(path).length === 0)
            return;
        root._request("set_path", { path: String(path) });
    }

    function _onSnapshot(payload) {
        var transition = payload.transition || {};
        var render = payload.render || {};

        root.directory = payload.directory || "";
        root.availability = payload.availability || "unavailable";
        root.availabilityReason = payload.availability_reason || "none";
        root.entries = payload.entries || [];
        root.current = payload.current || null;
        root.transitionType = transition.type || "fade";
        root.transitionDurationMs = typeof transition.duration_ms === "number"
            ? transition.duration_ms
            : 320;
        root.renderBackend = render.backend || "mpv";
        root.videoEnabled = typeof render.video_enabled === "boolean"
            ? render.video_enabled
            : true;
        root.videoAudio = typeof render.video_audio === "boolean"
            ? render.video_audio
            : false;
        root.ready = true;
        root.lastError = "";
        if (root.status !== "error")
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
                root.status = "ok";
        }
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected)
            root._onConnectionChanged(true);
    }
}
