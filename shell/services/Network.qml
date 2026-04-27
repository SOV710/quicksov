// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import Quickshell
import ".."
import "../ipc"
import "../ipc/protocol.js" as Protocol

Singleton {
    id: root

    property bool connected: false
    property bool ready: false
    property bool linkReady: false
    property bool wifiReady: false
    property string lastError: ""
    property string status: "disconnected"

    // net.link
    property var interfaces: []

    // net.wifi snapshot
    property string interfaceName: ""
    property string scanState: "idle"
    property double scanStartedAt: 0
    property double scanFinishedAt: 0
    property bool present: false
    property bool enabled: false
    property string availability: "unavailable"
    property string availabilityReason: "unknown"
    property string interfaceOperstate: ""
    property bool rfkillAvailable: false
    property bool rfkillSoftBlocked: false
    property bool rfkillHardBlocked: false
    property bool airplaneMode: false
    property bool wifiConnected: false
    property string ssid: ""
    property int signalDbm: 0
    property int signalPct: -1
    property int frequency: 0
    property var savedNetworks: []
    property var scanResults: []

    // local UI state
    property var pendingActions: ({})
    property string pendingConnectSsid: ""
    property double pendingConnectStartedAt: 0
    property bool pendingDisconnect: false
    property double pendingDisconnectStartedAt: 0

    readonly property bool wifiAvailable: availability !== "unavailable"
    // `scanPending` remains a short transport fallback until the daemon snapshot confirms state.
    readonly property bool scanPending: isPending("scan")
    readonly property bool scanRequestPending: scanPending && scanState === "idle"
    readonly property bool powerPending: isPending("power")
    readonly property bool airplanePending: isPending("airplane")
    readonly property bool scanning: scanState === "starting" || scanState === "running"
    readonly property bool isDisabled: availability === "disabled"
    readonly property bool isUnavailable: availability === "unavailable"
    readonly property bool wiredConnected: !!root._activeWiredInterface()
    readonly property bool anyConnected: wifiConnected || wiredConnected
    readonly property var wifiInterface: root._interfaceByName(interfaceName)
    readonly property var wiredInterface: root._activeWiredInterface()
    readonly property var currentInterface: wifiConnected ? wifiInterface : wiredInterface
    readonly property string currentIpv4: root._currentIpv4()
    readonly property var networks: root._mergeNetworks(scanResults, savedNetworks, ssid, wifiConnected)
    readonly property var currentNetworks: root.networks.filter(function(network) { return network.current; })
    readonly property var savedVisibleNetworks: root.networks.filter(function(network) {
        return !network.current && network.saved;
    })
    readonly property var availableNetworks: root.networks.filter(function(network) {
        return !network.current && !network.saved;
    })

    function _copyPending() {
        var copy = ({});
        for (var key in root.pendingActions)
            copy[key] = root.pendingActions[key];
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

    function _networkKey(ssid) {
        return "network:" + String(ssid || "");
    }

    function networkPending(ssid) {
        return root.isPending(root._networkKey(ssid))
            || root.pendingConnectSsid === String(ssid || "");
    }

    function networkPendingLabel(ssid) {
        if (root.isPending(root._networkKey(ssid)))
            return root.pendingActions[root._networkKey(ssid)];
        if (root.pendingConnectSsid === String(ssid || ""))
            return "Connecting";
        return "";
    }

    function canMutate() {
        return root.connected && root.ready;
    }

    function _setError(message) {
        root.lastError = message || "";
        root.status = root.lastError !== "" ? "error" : "ok";
        if (root.lastError !== "")
            errorClearTimer.restart();
        else
            errorClearTimer.stop();
    }

    function _handleReply(msg, pendingKey, options) {
        options = options || ({});

        if (pendingKey && !options.keepPendingOnSuccess)
            root._setPending(pendingKey, null);

        if (!msg)
            return;

        if (msg.kind === Protocol.Kind.ERR) {
            if (pendingKey)
                root._setPending(pendingKey, null);
            if (options.clearConnectOnError)
                root._clearConnectPending();
            if (options.clearDisconnectOnError)
                root._clearDisconnectPending();

            var body = msg.payload || {};
            root._setError(body.message || body.code || "Network request failed");
            return;
        }

        root._setError("");
    }

    function _request(action, payload, pendingKey, pendingValue, options) {
        options = options || ({});
        if (pendingKey && root.isPending(pendingKey))
            return;

        root._setError("");
        if (pendingKey)
            root._setPending(pendingKey, pendingValue || action);

        Client.request("net.wifi", action, payload, function(msg) {
            root._handleReply(msg, pendingKey, options);
        });
    }

    function _clearConnectPending() {
        root.pendingConnectSsid = "";
        root.pendingConnectStartedAt = 0;
    }

    function _clearDisconnectPending() {
        root.pendingDisconnect = false;
        root.pendingDisconnectStartedAt = 0;
    }

    function _onLinkSnapshot(payload) {
        root.interfaces = payload.interfaces || [];
        root.linkReady = true;
        root.ready = root.linkReady && root.wifiReady;
        if (root.status !== "error")
            root.status = "ok";
    }

    function _legacyConnectionState(state) {
        switch (state) {
        case "connected":
            return "connected";
        case "associating":
            return "associating";
        case "disconnected":
        case "scanning":
            return "disconnected";
        default:
            return "unknown";
        }
    }

    function _legacyScanState(state) {
        return state === "scanning" ? "running" : "idle";
    }

    function _onWifiSnapshot(payload) {
        var legacyState = payload.state || "unknown";
        var connectionState = payload.connection_state || root._legacyConnectionState(legacyState);

        root.interfaceName = payload.interface || "";
        root.scanState = payload.scan_state || root._legacyScanState(legacyState);
        root.scanStartedAt = typeof payload.scan_started_at === "number" ? payload.scan_started_at : 0;
        root.scanFinishedAt = typeof payload.scan_finished_at === "number" ? payload.scan_finished_at : 0;
        root.present = payload.present === true;
        root.enabled = payload.enabled === true;
        root.availability = payload.availability || "unavailable";
        root.availabilityReason = payload.availability_reason || "unknown";
        root.interfaceOperstate = payload.interface_operstate || "";
        root.rfkillAvailable = payload.rfkill_available === true;
        root.rfkillSoftBlocked = payload.rfkill_soft_blocked === true;
        root.rfkillHardBlocked = payload.rfkill_hard_blocked === true;
        root.airplaneMode = payload.airplane_mode === true;
        root.wifiConnected = connectionState === "connected";
        root.ssid = payload.ssid || "";
        root.signalDbm = typeof payload.rssi_dbm === "number" ? payload.rssi_dbm : 0;
        root.signalPct = typeof payload.signal_pct === "number" ? payload.signal_pct : -1;
        root.frequency = typeof payload.frequency === "number" ? payload.frequency : 0;
        root.savedNetworks = payload.saved_networks || [];
        root.scanResults = payload.scan_results || [];

        root.wifiReady = true;
        root.ready = root.linkReady && root.wifiReady;
        root._reconcileTransientState();
        if (root.status !== "error")
            root.status = "ok";
    }

    function _reconcileTransientState() {
        var now = Date.now();

        if (root.pendingConnectSsid !== "") {
            if (root.wifiConnected && root.ssid === root.pendingConnectSsid)
                root._clearConnectPending();
            else if (root.availability !== "ready")
                root._clearConnectPending();
            else if (now - root.pendingConnectStartedAt > 20000)
                root._clearConnectPending();
        }

        if (root.pendingDisconnect) {
            if (!root.wifiConnected)
                root._clearDisconnectPending();
            else if (now - root.pendingDisconnectStartedAt > 10000)
                root._clearDisconnectPending();
        }
    }

    function _interfaceByName(name) {
        if (!name)
            return null;
        for (var i = 0; i < root.interfaces.length; ++i) {
            var iface = root.interfaces[i];
            if (iface && iface.name === name)
                return iface;
        }
        return null;
    }

    function _isActiveInterface(iface) {
        if (!iface)
            return false;
        if (iface.operstate === "up")
            return true;
        if (iface.carrier === true && iface.kind !== "loopback")
            return true;
        if (iface.ipv4 && iface.ipv4.length > 0 && iface.kind !== "loopback")
            return true;
        return false;
    }

    function _activeWiredInterface() {
        for (var i = 0; i < root.interfaces.length; ++i) {
            var iface = root.interfaces[i];
            if (iface && iface.kind === "ethernet" && root._isActiveInterface(iface))
                return iface;
        }
        return null;
    }

    function _currentIpv4() {
        var iface = root.currentInterface;
        if (!iface || !iface.ipv4 || iface.ipv4.length === 0)
            return "";
        return String(iface.ipv4[0] || "");
    }

    function _securityLabel(flags) {
        var joined = "";
        for (var i = 0; i < flags.length; ++i)
            joined += String(flags[i]) + " ";

        if (joined.indexOf("SAE") >= 0 || joined.indexOf("WPA3") >= 0)
            return "WPA3";
        if (joined.indexOf("WPA2") >= 0 || joined.indexOf("RSN") >= 0)
            return "WPA2";
        if (joined.indexOf("WPA-") >= 0 || joined.indexOf("WPA ") >= 0 || joined.indexOf("WPA]") >= 0)
            return "WPA";
        if (joined.indexOf("OWE") >= 0)
            return "OWE";
        if (joined.indexOf("WEP") >= 0)
            return "WEP";
        if (joined.indexOf("802.1X") >= 0)
            return "802.1X";
        return "Open";
    }

    function _isSecure(flags) {
        return root._securityLabel(flags) !== "Open";
    }

    function _bandLabel(frequency) {
        if (!frequency || frequency <= 0)
            return "";
        if (frequency >= 5925)
            return "6 GHz";
        if (frequency >= 4900)
            return "5 GHz";
        if (frequency >= 2400)
            return "2.4 GHz";
        return "";
    }

    function _mergeNetworks(scanResults, savedNetworks, currentSsid, wifiConnected) {
        var bySsid = ({});

        function ensureNetwork(ssidValue) {
            if (!bySsid[ssidValue]) {
                bySsid[ssidValue] = {
                    ssid: ssidValue,
                    bssid: "",
                    rssiDbm: -100,
                    signalPct: -1,
                    frequency: 0,
                    bandLabel: "",
                    flags: [],
                    secure: false,
                    securityLabel: "Open",
                    saved: false,
                    current: false
                };
            }
            return bySsid[ssidValue];
        }

        for (var i = 0; i < scanResults.length; ++i) {
            var scan = scanResults[i];
            if (!scan || !scan.ssid)
                continue;

            var scanSsid = String(scan.ssid).trim();
            if (scanSsid.length === 0)
                continue;

            var existing = ensureNetwork(scanSsid);
            var pct = typeof scan.signal_pct === "number" ? scan.signal_pct : -1;
            if (pct >= existing.signalPct) {
                existing.bssid = scan.bssid || existing.bssid;
                existing.rssiDbm = typeof scan.rssi_dbm === "number" ? scan.rssi_dbm : existing.rssiDbm;
                existing.signalPct = pct;
                existing.frequency = typeof scan.frequency === "number" ? scan.frequency : existing.frequency;
                existing.bandLabel = root._bandLabel(existing.frequency);
                existing.flags = scan.flags || [];
                existing.securityLabel = root._securityLabel(existing.flags);
                existing.secure = root._isSecure(existing.flags);
            }
        }

        for (var j = 0; j < savedNetworks.length; ++j) {
            var saved = savedNetworks[j];
            if (!saved || !saved.ssid)
                continue;

            var savedSsid = String(saved.ssid).trim();
            if (savedSsid.length === 0)
                continue;

            ensureNetwork(savedSsid).saved = true;
        }

        if (wifiConnected && currentSsid && String(currentSsid).trim().length > 0)
            ensureNetwork(String(currentSsid).trim()).current = true;

        var list = [];
        for (var ssidKey in bySsid) {
            var network = bySsid[ssidKey];
            network.current = wifiConnected && currentSsid === network.ssid;
            list.push(network);
        }

        list.sort(function(a, b) {
            if (a.current !== b.current)
                return a.current ? -1 : 1;
            if (a.saved !== b.saved)
                return a.saved ? -1 : 1;
            if (a.signalPct !== b.signalPct)
                return b.signalPct - a.signalPct;
            return a.ssid.localeCompare(b.ssid);
        });

        return list;
    }

    function _currentNetwork() {
        for (var i = 0; i < root.networks.length; ++i) {
            if (root.networks[i].current)
                return root.networks[i];
        }
        return null;
    }

    function availabilityTitle() {
        switch (root.availabilityReason) {
        case "no_adapter":
            return "No Wi-Fi adapter";
        case "rfkill_soft_blocked":
            return root.airplaneMode ? "Airplane mode is on" : "Wi-Fi is off";
        case "rfkill_hard_blocked":
            return "Wi-Fi is hardware blocked";
        case "wpa_socket_missing":
            return "wpa_supplicant not available";
        case "permission_denied":
            return "Wi-Fi permission denied";
        case "backend_error":
            return "Wi-Fi backend unavailable";
        default:
            return root.availability === "ready" ? "Wi-Fi ready" : "Wi-Fi unavailable";
        }
    }

    function availabilityMessage() {
        switch (root.availabilityReason) {
        case "no_adapter":
            return "No wireless interface was found for the configured adapter.";
        case "rfkill_soft_blocked":
            return root.airplaneMode
                ? "Airplane mode is blocking wireless devices. Turn Flight off to scan or connect."
                : "Wi-Fi is soft blocked. Turn Wi-Fi on to scan nearby networks.";
        case "rfkill_hard_blocked":
            return "A hardware rfkill switch is blocking the adapter.";
        case "wpa_socket_missing":
            return "The wpa_supplicant control socket is missing or not started yet.";
        case "permission_denied":
            return "qsovd cannot access the wpa_supplicant control socket.";
        case "backend_error":
            return "The Wi-Fi backend returned an error. Check qsovd logs for details.";
        default:
            return "Wi-Fi status is unavailable right now.";
        }
    }

    function subtitle() {
        if (!root.ready)
            return "Waiting for daemon";

        if (root.availability === "disabled" || root.availability === "unavailable")
            return root.availabilityTitle();

        if (root.wifiConnected) {
            var parts = [root.ssid];
            if (root.signalPct >= 0)
                parts.push(String(root.signalPct) + "%");
            if (root.currentIpv4 !== "")
                parts.push(root.currentIpv4);
            if (root.scanning)
                parts.push("scanning");
            return parts.join(" • ");
        }

        if (root.wiredConnected) {
            var wired = ["Ethernet"];
            if (root.currentIpv4 !== "")
                wired.push(root.currentIpv4);
            if (root.scanning)
                wired.push("scanning");
            return wired.join(" • ");
        }

        if (root.scanning)
            return "Scanning nearby networks";

        if (root.networks.length > 0)
            return String(root.networks.length) + " networks available";

        return "Ready";
    }

    function iconPathForSignal(signalPct) {
        return Theme.wifiIconForSignal(signalPct);
    }

    function networkIconPath(network) {
        if (!network)
            return Theme.iconWifiZeroStatus;
        return root.iconPathForSignal(network.signalPct);
    }

    function networkSubtitle(network) {
        if (!network)
            return "";

        var parts = [];
        if (network.current)
            parts.push("Connected");
        else if (network.saved)
            parts.push("Saved");
        else
            parts.push(network.secure ? network.securityLabel : "Open");

        if (!network.current && network.secure)
            parts.push(network.securityLabel);
        if (network.bandLabel)
            parts.push(network.bandLabel);
        if (network.signalPct >= 0)
            parts.push(String(network.signalPct) + "%");

        return parts.join(" • ");
    }

    function toggleEnabled() {
        if (!root.present || !root.rfkillAvailable || root.powerPending)
            return;
        root._request(
            "set_enabled",
            { enabled: !root.enabled },
            "power",
            root.enabled ? "Turning off" : "Turning on"
        );
    }

    function toggleAirplaneMode() {
        if (!root.rfkillAvailable || root.airplanePending)
            return;
        root._request(
            "set_airplane_mode",
            { enabled: !root.airplaneMode },
            "airplane",
            root.airplaneMode ? "Turning off" : "Turning on"
        );
    }

    function scan() {
        if (!root.canMutate() || root.availability !== "ready" || root.scanState !== "idle" || root.scanRequestPending)
            return;
        root._request("scan", {}, "scan", "Scanning");
    }

    function maybeRefreshScan() {
        if (!root.canMutate() || root.availability !== "ready" || root.scanState !== "idle" || root.scanRequestPending)
            return;

        var lastScanAt = root.scanFinishedAt > 0 ? root.scanFinishedAt : root.scanStartedAt;
        var stale = lastScanAt <= 0 || (Date.now() - lastScanAt) > 15000;
        if (root.scanResults.length === 0 || stale)
            root.scan();
    }

    function connectTo(network, psk, save) {
        if (!network || !network.ssid || !root.canMutate() || root.availability !== "ready")
            return;

        var payload = { ssid: network.ssid, save: save === true };
        if (psk && String(psk).length > 0)
            payload.psk = String(psk);

        root.pendingConnectSsid = network.ssid;
        root.pendingConnectStartedAt = Date.now();
        root._request(
            "connect",
            payload,
            root._networkKey(network.ssid),
            "Connecting",
            { keepPendingOnSuccess: false, clearConnectOnError: true }
        );
        root._setPending(root._networkKey(network.ssid), null);
    }

    function disconnectCurrent() {
        if (!root.wifiConnected || root.pendingDisconnect || !root.canMutate())
            return;

        root.pendingDisconnect = true;
        root.pendingDisconnectStartedAt = Date.now();
        root._request(
            "disconnect",
            {},
            "disconnect",
            "Disconnecting",
            { clearDisconnectOnError: true }
        );
        root._setPending("disconnect", null);
    }

    function forgetNetwork(network) {
        if (!network || !network.ssid || !network.saved || !root.canMutate())
            return;

        root._request(
            "forget",
            { ssid: network.ssid },
            root._networkKey(network.ssid),
            "Forgetting"
        );
    }

    function primaryActionLabel(network) {
        if (!network)
            return "";
        if (root.isPending(root._networkKey(network.ssid)))
            return root.pendingActions[root._networkKey(network.ssid)];
        if (root.pendingConnectSsid === network.ssid)
            return "Connecting";
        if (network.current)
            return root.pendingDisconnect ? "Disconnecting" : "Disconnect";
        return "Connect";
    }

    function canConnect(network) {
        if (!network || !root.canMutate())
            return false;
        if (root.availability !== "ready")
            return false;
        if (root.isPending(root._networkKey(network.ssid)))
            return false;
        if (root.pendingDisconnect)
            return false;
        if (root.pendingConnectSsid !== "" && root.pendingConnectSsid !== network.ssid)
            return false;
        return true;
    }

    function _onConnectionChanged(isConnected) {
        root.connected = isConnected;
        if (isConnected) {
            Client.subscribe("net.link", root._onLinkSnapshot);
            Client.subscribe("net.wifi", root._onWifiSnapshot);
        } else {
            root.ready = false;
            root.linkReady = false;
            root.wifiReady = false;
            root.status = "disconnected";
            root.lastError = "";
            root.interfaces = [];
            root.interfaceName = "";
            root.scanState = "idle";
            root.scanStartedAt = 0;
            root.scanFinishedAt = 0;
            root.present = false;
            root.enabled = false;
            root.availability = "unavailable";
            root.availabilityReason = "unknown";
            root.interfaceOperstate = "";
            root.rfkillAvailable = false;
            root.rfkillSoftBlocked = false;
            root.rfkillHardBlocked = false;
            root.airplaneMode = false;
            root.wifiConnected = false;
            root.ssid = "";
            root.signalDbm = 0;
            root.signalPct = -1;
            root.frequency = 0;
            root.savedNetworks = [];
            root.scanResults = [];
            root.pendingActions = ({});
            root._clearConnectPending();
            root._clearDisconnectPending();
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
        interval: 500
        repeat: true
        running: true
        onTriggered: root._reconcileTransientState()
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onConnectionChanged);
        if (Client.connected)
            root._onConnectionChanged(true);
    }
}
