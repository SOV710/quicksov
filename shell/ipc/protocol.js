// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

.pragma library

var Kind = {
    REQ:     0,
    REP:     1,
    ERR:     2,
    PUB:     3,
    ONESHOT: 4,
    SUB:     5,
    UNSUB:   6,
    SUB_EVENTS: 7,
    UNSUB_EVENTS: 8
};

var _nextId = 1;

function nextId() { return _nextId++; }

function makeHello() {
    return { proto_version: "qsov/1", client_name: "quickshell", client_version: "0.1.0" };
}

function makeSub(topic) {
    return { id: 0, kind: Kind.SUB, topic: topic, action: "", payload: {} };
}

function makeUnsub(topic) {
    return { id: 0, kind: Kind.UNSUB, topic: topic, action: "", payload: {} };
}

function makeSubEvents(topic) {
    return { id: 0, kind: Kind.SUB_EVENTS, topic: topic, action: "", payload: {} };
}

function makeUnsubEvents(topic) {
    return { id: 0, kind: Kind.UNSUB_EVENTS, topic: topic, action: "", payload: {} };
}

function makeReq(topic, action, payload) {
    var id = nextId();
    return { id: id, kind: Kind.REQ, topic: topic, action: action, payload: payload || {} };
}

function isError(msg) { return msg && msg.kind === Kind.ERR; }
function isPub(msg)   { return msg && msg.kind === Kind.PUB; }
function isRep(msg)   { return msg && msg.kind === Kind.REP; }
