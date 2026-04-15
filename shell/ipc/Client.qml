// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import QtCore
import Quickshell
import Quickshell.Io
import "protocol.js" as Protocol

Singleton {
    id: root

    property bool connected: false
    property bool handshakeDone: false
    property string sessionId: ""
    property var capabilities: []

    property int _retryDelay: 500
    readonly property int _maxDelay: 5000

    property var _pending: ({})
    property var _subscribers: ({})

    signal connectionChanged(bool isConnected)
    signal pubReceived(string topic, var payload)

    property var _socket: Socket {
        id: socket
        path: root._socketPath()

        parser: SplitParser {
            splitMarker: "\n"
            onRead: function(data) {
                if (!data || data.trim() === "") return;
                try {
                    root._handleMessage(JSON.parse(data));
                } catch(e) {
                    console.warn("[ipc] parse error:", e, data);
                }
            }
        }

        onConnectedChanged: {
            if (!connected) {
                root.handshakeDone = false;
                root.connectionChanged(false);
                root._scheduleReconnect();
            } else {
                // Send Hello immediately on connect
                var hello = Protocol.makeHello();
                socket.write(JSON.stringify(hello) + "\n");
                socket.flush();
            }
        }

        onError: function(error) {
            console.warn("[ipc] socket error:", error);
            // onConnectedChanged may not fire if connected was already false
            if (!reconnectTimer.running) root._scheduleReconnect();
        }
    }

    property var _reconnectTimer: Timer {
        id: reconnectTimer
        interval: 500
        repeat: false
        onTriggered: root._connect()
    }

    function _socketPath() {
        // StandardPaths.writableLocation returns a file:// URL; strip the scheme
        var rtUrl = StandardPaths.writableLocation(StandardPaths.RuntimeLocation).toString();
        var rtDir = rtUrl.replace(/^file:\/\//, "");
        var path = rtDir + "/quicksov/daemon.sock";
        console.log("[ipc] socket path:", path);
        return path;
    }

    function _connect() {
        socket.connected = true;
    }

    function _scheduleReconnect() {
        if (reconnectTimer.running) return;
        reconnectTimer.interval = root._retryDelay;
        reconnectTimer.start();
        root._retryDelay = Math.min(root._retryDelay * 2, root._maxDelay);
    }

    function _resetBackoff() { root._retryDelay = 500; }

    function _handleMessage(msg) {
        if (msg._type === "HelloAck") {
            root.connected = true;
            root.handshakeDone = true;
            root.sessionId = msg.session_id != null ? String(msg.session_id) : "";
            root.capabilities = msg.capabilities || [];
            root._resetBackoff();
            root.connectionChanged(true);
            var topics = Object.keys(root._subscribers);
            for (var i = 0; i < topics.length; i++) {
                root._sendRaw(Protocol.makeSub(topics[i]));
            }
            return;
        }
        if (!root.handshakeDone) return;

        if (msg.kind === 3) {
            var subs = root._subscribers[msg.topic];
            if (subs) {
                for (var j = 0; j < subs.length; j++) subs[j](msg.payload);
            }
            root.pubReceived(msg.topic, msg.payload);
        } else if (msg.kind === 1 || msg.kind === 2) {
            var cb = root._pending[msg.id];
            if (cb) {
                delete root._pending[msg.id];
                cb(msg);
            }
        }
    }

    function _sendRaw(obj) {
        if (!socket.connected) return;
        socket.write(JSON.stringify(obj) + "\n");
        socket.flush();
    }

    function subscribe(topic, callback) {
        if (!root._subscribers[topic]) root._subscribers[topic] = [];
        root._subscribers[topic].push(callback);
        if (root.handshakeDone) root._sendRaw(Protocol.makeSub(topic));
    }

    function unsubscribe(topic, callback) {
        var subs = root._subscribers[topic];
        if (!subs) return;
        var idx = subs.indexOf(callback);
        if (idx >= 0) subs.splice(idx, 1);
        if (subs.length === 0) {
            delete root._subscribers[topic];
            if (root.handshakeDone) root._sendRaw(Protocol.makeUnsub(topic));
        }
    }

    function request(topic, action, payload, callback) {
        var msg = Protocol.makeReq(topic, action, payload);
        if (callback) root._pending[msg.id] = callback;
        root._sendRaw(msg);
        return msg.id;
    }

    Component.onCompleted: root._connect()
}
