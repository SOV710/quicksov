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
    property string currentTimeIso: ""
    property string timezoneAbbreviation: ""

    function _normalizedIconName(code) {
        if (code === 0)
            return "sun";
        if (code >= 1 && code <= 3)
            return "cloud-sun";
        if (code === 45 || code === 48)
            return "cloud-fog";
        if ((code >= 51 && code <= 57))
            return "cloud-drizzle";
        if ((code >= 61 && code <= 67) || (code >= 80 && code <= 82))
            return "cloud-rain";
        if ((code >= 71 && code <= 77) || code === 85 || code === 86)
            return "cloud-snow";
        if (code === 95 || code === 96 || code === 99)
            return "cloud-lightning";
        return "cloud";
    }

    function iconNameForWmo(code) {
        if (typeof code !== "number")
            return "cloud";
        return root._normalizedIconName(code);
    }

    function iconPathForWmo(code) {
        return "lucide/" + root.iconNameForWmo(code) + ".svg";
    }

    function isExpired(nowMs) {
        if (root.lastSuccessMs === null || root.ttlSec <= 0)
            return false;
        var at = typeof nowMs === "number" ? nowMs : Date.now();
        return (at - root.lastSuccessMs) > (root.ttlSec * 1000);
    }

    function hasUsableSnapshot(nowMs) {
        return root.current !== null
            && root.hourlyForecast.length > 0
            && !root.isExpired(nowMs);
    }

    function refresh() {
        Client.request("weather", "refresh", {}, null);
    }

    function _onSnapshot(payload) {
        root.provider       = payload.provider || "";
        root.fetchStatus    = payload.status || "loading";
        root.ttlSec         = typeof payload.ttl_sec === "number" ? payload.ttl_sec : 0;
        root.location       = payload.location || null;
        root.current        = payload.current || null;
        root.currentTimeIso = root.current && typeof root.current.time === "string"
            ? root.current.time
            : "";
        root.timezoneAbbreviation = root.current && typeof root.current.timezone_abbreviation === "string"
            ? root.current.timezone_abbreviation
            : "";
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
            root.currentTimeIso = "";
            root.timezoneAbbreviation = "";
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
