// SPDX-FileCopyrightText: 2026 SOV710
//
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
    property var _eventSubscribers: ({})

    signal connectionChanged(bool isConnected)
    signal pubReceived(string topic, var payload)
    signal eventReceived(string topic, string eventName, var payload)

    property var _socket: socketLoader.item

    Loader {
        id: socketLoader
        active: true
        sourceComponent: socketComponent
    }

    Component {
        id: socketComponent

        Socket {
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
                    root._markDisconnected();
                    root._ensureReconnectLoop();
                } else {
                    root._sendHello();
                }
            }

            onError: function(error) {
                console.warn("[ipc] socket error:", error);
                // onConnectedChanged may not fire if connected was already false
                root._markDisconnected();
                root._ensureReconnectLoop();
            }
        }
    }

    property var _reconnectTimer: Timer {
        id: reconnectTimer
        interval: 500
        repeat: false
        onTriggered: root._reconnectTick()
    }

    function _socketPath() {
        var override = Quickshell.env("QSOV_SOCKET");
        if (override && String(override).length > 0) {
            var overridePath = String(override);
            console.log("[ipc] socket path:", overridePath);
            return overridePath;
        }

        // StandardPaths.writableLocation returns a file:// URL; strip the scheme
        var rtDir = Quickshell.env("XDG_RUNTIME_DIR");
        if (!rtDir || String(rtDir).length === 0) {
            var rtUrl = StandardPaths.writableLocation(StandardPaths.RuntimeLocation).toString();
            rtDir = rtUrl.replace(/^file:\/\//, "");
        }
        var path = rtDir + "/quicksov/daemon.sock";
        console.log("[ipc] socket path:", path);
        return path;
    }

    function _connect() {
        var socket = root._recreateSocket();
        if (!socket) return;
        socket.connected = true;
    }

    function _recreateSocket() {
        if (socketLoader.active) socketLoader.active = false;
        socketLoader.active = true;
        return root._socket;
    }

    function _sendHello() {
        var socket = root._socket;
        if (!socket) return;
        root.handshakeDone = false;
        var hello = Protocol.makeHello();
        socket.write(JSON.stringify(hello) + "\n");
        socket.flush();
    }

    function _markDisconnected() {
        var wasConnected = root.connected || root.handshakeDone;
        root.connected = false;
        root.handshakeDone = false;
        root.sessionId = "";
        root.capabilities = [];
        root._pending = ({});
        if (wasConnected) root.connectionChanged(false);
    }

    function _ensureReconnectLoop() {
        if (reconnectTimer.running) return;
        reconnectTimer.interval = root._retryDelay;
        reconnectTimer.start();
    }

    function _reconnectTick() {
        if (root.handshakeDone) return;

        root._connect();

        var nextDelay = Math.min(root._retryDelay * 2, root._maxDelay);
        reconnectTimer.interval = nextDelay;
        reconnectTimer.start();
        root._retryDelay = nextDelay;
    }

    function _resetBackoff() { root._retryDelay = 500; }

    function _handleMessage(msg) {
        if (msg._type === "HelloAck") {
            if (reconnectTimer.running) reconnectTimer.stop();
            root.connected = true;
            root.handshakeDone = true;
            root.sessionId = msg.session_id != null ? String(msg.session_id) : "";
            root.capabilities = msg.capabilities || [];
            root._resetBackoff();
            var topics = Object.keys(root._subscribers);
            for (var i = 0; i < topics.length; i++) {
                root._sendRaw(Protocol.makeSub(topics[i]));
            }
            var eventTopics = Object.keys(root._eventSubscribers);
            for (var j = 0; j < eventTopics.length; j++) {
                root._sendRaw(Protocol.makeSubEvents(eventTopics[j]));
            }
            root.connectionChanged(true);
            return;
        }
        if (!root.handshakeDone) return;

        if (msg.kind === 3) {
            if (msg.action && msg.action.length > 0) {
                var eventSubs = root._eventSubscribers[msg.topic];
                if (eventSubs) {
                    for (var j = 0; j < eventSubs.length; j++) eventSubs[j](msg.action, msg.payload);
                }
                root.eventReceived(msg.topic, msg.action, msg.payload);
            } else {
                var subs = root._subscribers[msg.topic];
                if (subs) {
                    for (var k = 0; k < subs.length; k++) subs[k](msg.payload);
                }
                root.pubReceived(msg.topic, msg.payload);
            }
        } else if (msg.kind === 1 || msg.kind === 2) {
            var cb = root._pending[msg.id];
            if (cb) {
                delete root._pending[msg.id];
                cb(msg);
            }
        }
    }

    function _sendRaw(obj) {
        var socket = root._socket;
        if (!socket) return;
        if (!socket.connected) return;
        socket.write(JSON.stringify(obj) + "\n");
        socket.flush();
    }

    function subscribe(topic, callback) {
        if (!root._subscribers[topic]) root._subscribers[topic] = [];
        var subs = root._subscribers[topic];
        if (subs.indexOf(callback) >= 0) return;

        var wasEmpty = subs.length === 0;
        subs.push(callback);
        if (wasEmpty && root.handshakeDone) root._sendRaw(Protocol.makeSub(topic));
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

    function subscribeEvents(topic, callback) {
        if (!root._eventSubscribers[topic]) root._eventSubscribers[topic] = [];
        var subs = root._eventSubscribers[topic];
        if (subs.indexOf(callback) >= 0) return;

        var wasEmpty = subs.length === 0;
        subs.push(callback);
        if (wasEmpty && root.handshakeDone) root._sendRaw(Protocol.makeSubEvents(topic));
    }

    function unsubscribeEvents(topic, callback) {
        var subs = root._eventSubscribers[topic];
        if (!subs) return;
        var idx = subs.indexOf(callback);
        if (idx >= 0) subs.splice(idx, 1);
        if (subs.length === 0) {
            delete root._eventSubscribers[topic];
            if (root.handshakeDone) root._sendRaw(Protocol.makeUnsubEvents(topic));
        }
    }

    function request(topic, action, payload, callback) {
        var msg = Protocol.makeReq(topic, action, payload);
        if (callback) root._pending[msg.id] = callback;
        root._sendRaw(msg);
        return msg.id;
    }

    Component.onCompleted: {
        root._ensureReconnectLoop();
        root._connect();
    }
}
