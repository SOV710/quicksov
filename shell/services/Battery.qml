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

    property string availability: "backend_unavailable"
    property bool present: false
    // "charging" | "discharging" | "fully_charged" | "not_charging" | "empty" | "unknown"
    property string chargeStatus: ""
    property real percentage: 0.0
    property bool onBattery: false
    property var timeToEmptySec: null
    property var timeToFullSec: null
    property var batteries: []
    property string powerProfile: ""
    property bool powerProfileAvailable: false
    property string powerProfileBackend: "none"
    property var powerProfileReason: null
    property var powerProfileChoices: []
    property var healthPercent: null
    property var energyRateW: null
    property var energyNowWh: null
    property var energyFullWh: null
    property var energyDesignWh: null
    property string pendingProfile: ""

    readonly property bool profilePending: pendingProfile !== ""
    readonly property bool isUnavailable: availability === "backend_unavailable"
    readonly property var presentBatteries: batteries.filter(function(entry) {
        return entry && entry.present === true;
    })
    readonly property bool hasBattery: availability === "ready" && presentBatteries.length > 0
    readonly property bool noBattery: availability === "no_battery" || (!present && !isUnavailable)
    readonly property bool isCharging: chargeStatus === "charging"
    readonly property bool isFullyCharged: chargeStatus === "fully_charged"

    function _setError(message) {
        root.lastError = message || "";
        root.status = root.lastError !== "" ? "error" : "ok";
        if (root.lastError !== "")
            errorClearTimer.restart();
        else
            errorClearTimer.stop();
    }

    function _resetState() {
        root.availability = "backend_unavailable";
        root.present = false;
        root.chargeStatus = "";
        root.percentage = 0.0;
        root.onBattery = false;
        root.timeToEmptySec = null;
        root.timeToFullSec = null;
        root.batteries = [];
        root.powerProfile = "";
        root.powerProfileAvailable = false;
        root.powerProfileBackend = "none";
        root.powerProfileReason = null;
        root.powerProfileChoices = [];
        root.healthPercent = null;
        root.energyRateW = null;
        root.energyNowWh = null;
        root.energyFullWh = null;
        root.energyDesignWh = null;
        root.pendingProfile = "";
        root.lastError = "";
        profilePendingClearTimer.stop();
    }

    function _formatDuration(seconds) {
        if (typeof seconds !== "number" || seconds <= 0)
            return "";

        var totalMinutes = Math.round(seconds / 60);
        var hours = Math.floor(totalMinutes / 60);
        var minutes = totalMinutes % 60;

        if (hours > 0)
            return String(hours) + "h " + String(minutes) + "m";
        return String(minutes) + "m";
    }

    function displayStatus() {
        switch (root.chargeStatus) {
        case "charging":
            return "Charging";
        case "discharging":
            return "Discharging";
        case "fully_charged":
            return "Fully charged";
        case "not_charging":
            return "Not charging";
        case "empty":
            return "Empty";
        default:
            return "Unknown";
        }
    }

    function timeEstimateText() {
        if (!root.hasBattery)
            return root.noBattery ? "No battery detected" : "Battery backend unavailable";

        if (root.chargeStatus === "charging" && typeof root.timeToFullSec === "number" && root.timeToFullSec > 0)
            return root._formatDuration(root.timeToFullSec) + " until full";
        if (root.onBattery && typeof root.timeToEmptySec === "number" && root.timeToEmptySec > 0)
            return root._formatDuration(root.timeToEmptySec) + " remaining";
        if (root.chargeStatus === "fully_charged")
            return "Connected to AC power";
        return "Time estimate unavailable";
    }

    function sourceLabel() {
        if (!root.hasBattery)
            return root.noBattery ? "No battery" : "Unavailable";
        return root.onBattery ? "Battery" : "AC";
    }

    function profileLabel(profile) {
        switch (profile) {
        case "power-saver":
            return "Saver";
        case "balanced":
            return "Balanced";
        case "performance":
            return "Performance";
        case "custom":
            return "Custom";
        default:
            return "Unknown";
        }
    }

    function canSetPowerProfile(profile) {
        return root.connected
            && root.ready
            && root.powerProfileAvailable
            && root.powerProfileChoices.indexOf(profile) >= 0
            && !root.profilePending
            && root.powerProfile !== profile;
    }

    function setPowerProfile(profile) {
        if (!root.canSetPowerProfile(profile))
            return;

        root.pendingProfile = profile;
        profilePendingClearTimer.restart();
        root._setError("");
        Client.request("battery", "set_power_profile", { profile: profile }, function(msg) {
            if (!msg)
                return;
            if (msg.kind === Protocol.Kind.ERR) {
                profilePendingClearTimer.stop();
                root.pendingProfile = "";
                var body = msg.payload || {};
                root._setError(body.message || body.code || "Battery request failed");
                return;
            }
            root._setError("");
        });
    }

    function _onSnapshot(payload) {
        root.availability   = payload.availability       || "backend_unavailable";
        root.present        = payload.present            || false;
        root.chargeStatus   = payload.state              || "";
        root.percentage     = payload.level              || 0.0;
        root.onBattery      = payload.on_battery         || false;
        root.timeToEmptySec = payload.time_to_empty_sec;
        root.timeToFullSec  = payload.time_to_full_sec;
        root.batteries      = Array.isArray(payload.batteries) ? payload.batteries : [];
        root.powerProfile   = payload.power_profile      || "";
        root.powerProfileAvailable = payload.power_profile_available === true;
        root.powerProfileBackend = payload.power_profile_backend || "none";
        root.powerProfileReason = typeof payload.power_profile_reason === "string"
                                  ? payload.power_profile_reason
                                  : null;
        root.powerProfileChoices = Array.isArray(payload.power_profile_choices)
                                   ? payload.power_profile_choices
                                   : [];
        root.healthPercent  = typeof payload.health_percent === "number" ? payload.health_percent : null;
        root.energyRateW    = typeof payload.energy_rate_w === "number" ? payload.energy_rate_w : null;
        root.energyNowWh    = typeof payload.energy_now_wh === "number" ? payload.energy_now_wh : null;
        root.energyFullWh   = typeof payload.energy_full_wh === "number" ? payload.energy_full_wh : null;
        root.energyDesignWh = typeof payload.energy_design_wh === "number" ? payload.energy_design_wh : null;
        root.ready  = true;
        if (root.pendingProfile !== "" && root.powerProfile === root.pendingProfile) {
            profilePendingClearTimer.stop();
            root.pendingProfile = "";
        }
        if (root.status !== "error")
            root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("battery", root._onSnapshot);
        } else {
            root._resetState();
            root.ready  = false;
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

    Timer {
        id: profilePendingClearTimer
        interval: 5000
        repeat: false
        onTriggered: root.pendingProfile = ""
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected) root._onConnectionChanged(true);
    }
}
