// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import Quickshell

Singleton {
    id: root

    // Keep diagnostics opt-in so normal interaction stays production-like.
    property real slowMotionFactor: 1.0
    property bool logRevealGeometry: false
    property bool disablePanelContentClip: false
    property bool disableToastClip: false
    property bool disableToastEnterHeightAnimation: false
    property bool freezePanelBodyHeightToFinal: false
    property bool disablePanelShell: false
    property bool disablePanelBlurRegion: false
    property bool forceCaptureMaskWhilePopupOpen: false
    property bool forceShellRegionMaskWhilePopupOpen: false
    property bool forceZeroBodyRadius: false
    property bool disableRegionRounding: false

    function duration(baseDuration) {
        return Math.max(0, Math.round(baseDuration * Math.max(0.01, root.slowMotionFactor)));
    }

    function _roundValue(value) {
        if (typeof value === "number")
            return Math.round(value * 100) / 100;
        return value;
    }

    function _formatFields(fields) {
        if (!fields)
            return "";

        var keys = Object.keys(fields).sort();
        var parts = [];
        for (var i = 0; i < keys.length; ++i) {
            var key = keys[i];
            parts.push(key + "=" + _roundValue(fields[key]));
        }
        return parts.join(" ");
    }

    function logTransition(surfaceName, phase, fields) {
        if (!root.logRevealGeometry)
            return;

        var prefix = "[debug-visuals][" + (surfaceName || "unknown") + "][" + (phase || "event") + "]";
        var suffix = _formatFields(fields);
        console.log(suffix !== "" ? prefix + " " + suffix : prefix);
    }
}
