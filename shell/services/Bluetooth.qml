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

    property bool btAvailable: false
    property bool btEnabled: false
    property bool discovering: false
    property var devices: []
    property var pendingActions: ({})

    readonly property var sortedDevices: root._sortDevices(root.devices)
    readonly property var connectedDevices: root.sortedDevices.filter(function(d) { return d.connected; })
    readonly property var pairedDevices: root.sortedDevices.filter(function(d) { return !d.connected && d.paired; })
    readonly property var availableDevices: root.sortedDevices.filter(function(d) { return !d.connected && !d.paired; })
    readonly property bool scanPending: root.isPending("scan")
    readonly property bool powerPending: root.isPending("power")
    readonly property string scanBlockedReason: root._scanBlockedReason()
    readonly property bool scanBlocked: root.scanBlockedReason !== ""

    function _copyPending() {
        var copy = ({});
        for (var key in root.pendingActions) copy[key] = root.pendingActions[key];
        return copy;
    }

    function _setPending(key, value) {
        var next = root._copyPending();
        if (value === undefined || value === null || value === "")
            delete next[key];
        else
            next[key] = value;
        root.pendingActions = next;
    }

    function isPending(key) {
        return root.pendingActions[key] !== undefined;
    }

    function _deviceKey(address) {
        return "device:" + String(address || "");
    }

    function devicePending(address) {
        return root.isPending(root._deviceKey(address));
    }

    function devicePendingAction(address) {
        return root.pendingActions[root._deviceKey(address)] || "";
    }

    function _scanBlockedReason() {
        for (var key in root.pendingActions) {
            var action = root.pendingActions[key];
            if (action === "Connecting") return "Scan paused while connecting";
            if (action === "Pairing") return "Scan paused while pairing";
        }
        return "";
    }

    function _setError(message) {
        root.lastError = message || "";
        root.status = root.lastError !== "" ? "error" : "ok";
        if (root.lastError !== "")
            errorClearTimer.restart();
        else
            errorClearTimer.stop();
    }

    function _handleReply(msg, pendingKey) {
        if (pendingKey) root._setPending(pendingKey, null);

        if (!msg) return;
        if (msg.kind === Protocol.Kind.ERR) {
            var body = msg.payload || {};
            root._setError(body.message || body.code || "Bluetooth request failed");
            return;
        }

        root._setError("");
    }

    function _request(action, payload, pendingKey, pendingValue) {
        if (pendingKey && root.isPending(pendingKey)) return;
        root._setError("");
        if (pendingKey) root._setPending(pendingKey, pendingValue || action);
        Client.request("bluetooth", action, payload, function(msg) {
            root._handleReply(msg, pendingKey);
        });
    }

    function _displayName(device) {
        if (!device) return "Unknown device";

        var name = device.name ? String(device.name).trim() : "";
        if (name.length > 0) return name;

        var address = device.address ? String(device.address).trim() : "";
        if (address.length > 0) return address;

        return "Unknown device";
    }

    function _sortDevices(devices) {
        var list = (devices || []).slice();
        list.sort(function(a, b) {
            function rank(device) {
                if (device.connected) return 0;
                if (device.paired) return 1;
                return 2;
            }

            var rankDiff = rank(a) - rank(b);
            if (rankDiff !== 0) return rankDiff;

            var aName = root._displayName(a).toLowerCase();
            var bName = root._displayName(b).toLowerCase();
            var aAddr = a.address ? String(a.address).toLowerCase() : "";
            var bAddr = b.address ? String(b.address).toLowerCase() : "";
            var aIsAddress = aName === aAddr && aAddr.length > 0;
            var bIsAddress = bName === bAddr && bAddr.length > 0;

            if (aIsAddress !== bIsAddress) return aIsAddress ? 1 : -1;
            return aName.localeCompare(bName);
        });
        return list;
    }

    function deviceLabel(device) {
        return root._displayName(device);
    }

    function deviceStatus(device) {
        if (!device) return "";

        var parts = [];
        if (device.connected) parts.push("Connected");
        else if (device.paired) parts.push("Paired");
        else parts.push("Available");

        if (device.battery !== null && device.battery !== undefined)
            parts.push(String(device.battery) + "%");

        return parts.join(" • ");
    }

    function setPowered(on) {
        root._request("power", { on: on }, "power", on ? "Turning on" : "Turning off");
    }

    function togglePowered() {
        if (!root.btAvailable || root.powerPending) return;
        root.setPowered(!root.btEnabled);
    }

    function startScan() {
        if (!root.btAvailable || !root.btEnabled || root.scanPending || root.scanBlocked) return;
        root._request("scan_start", {}, "scan", "Starting scan");
    }

    function stopScan() {
        if (!root.btAvailable || !root.btEnabled || root.scanPending || root.scanBlocked) return;
        root._request("scan_stop", {}, "scan", "Stopping scan");
    }

    function toggleScan() {
        if (root.discovering) root.stopScan();
        else root.startScan();
    }

    function connectDevice(address) {
        if (!address || root.devicePending(address)) return;
        root._request("connect", { address: address }, root._deviceKey(address), "Connecting");
    }

    function disconnectDevice(address) {
        if (!address || root.devicePending(address)) return;
        root._request("disconnect", { address: address }, root._deviceKey(address), "Disconnecting");
    }

    function pairDevice(address) {
        if (!address || root.devicePending(address)) return;
        root._request("pair", { address: address }, root._deviceKey(address), "Pairing");
    }

    function forgetDevice(address) {
        if (!address || root.devicePending(address)) return;
        root._request("forget", { address: address }, root._deviceKey(address), "Forgetting");
    }

    function _onSnapshot(payload) {
        root.btAvailable = payload.available  || false;
        root.btEnabled   = payload.powered    || false;
        root.discovering = payload.discovering || false;
        root.devices     = payload.devices    || [];
        root.ready  = true;
        if (root.status !== "error") root.status = "ok";
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("bluetooth", root._onSnapshot);
        } else {
            root.btAvailable = false;
            root.btEnabled = false;
            root.discovering = false;
            root.devices = [];
            root.pendingActions = ({});
            root.lastError = "";
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

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected) root._onConnectionChanged(true);
    }
}
