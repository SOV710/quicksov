// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQml

QtObject {
    id: root

    property string activePopup: ""

    readonly property bool anyOpen: activePopup !== ""
    readonly property bool clockOpen: activePopup === "clock"
    readonly property string statusPopup: _isStatusPopup(activePopup) ? activePopup : ""

    function _isStatusPopup(name) {
        switch (name) {
        case "battery":
        case "network":
        case "bluetooth":
        case "volume":
        case "notification":
            return true;
        default:
            return false;
        }
    }

    function toggle(name) {
        root.activePopup = root.activePopup === name ? "" : name;
    }

    function open(name) {
        root.activePopup = name;
    }

    function close() {
        root.activePopup = "";
    }
}
