// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQml
import ".."

QtObject {
    id: root

    property string activePopup: ""
    property string lastActivePopup: ""
    property string _previousActivePopup: ""

    readonly property bool anyOpen: activePopup !== ""
    readonly property bool clockOpen: activePopup === "clock"
    readonly property string statusPopup: _isStatusPopup(activePopup) ? activePopup : ""
    readonly property string statusPopupLabel: _isStatusPopup(activePopup)
                                               ? activePopup
                                               : (_isStatusPopup(lastActivePopup) ? lastActivePopup : "status")

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

    onActivePopupChanged: {
        var previous = root._previousActivePopup;
        if (previous === root.activePopup)
            return;

        if (previous !== "") {
            DebugVisuals.logTransition(previous, "popup-close", {
                event: root.activePopup === "" ? "requested-close" : "switch",
                next: root.activePopup !== "" ? root.activePopup : "none"
            });
        }

        if (root.activePopup !== "") {
            root.lastActivePopup = root.activePopup;
            DebugVisuals.logTransition(root.activePopup, "popup-open", {
                event: previous === "" ? "requested-open" : "switch",
                previous: previous !== "" ? previous : "none"
            });
        }

        root._previousActivePopup = root.activePopup;
    }

    Component.onCompleted: {
        root._previousActivePopup = root.activePopup;
        if (root.activePopup !== "")
            root.lastActivePopup = root.activePopup;
    }
}
