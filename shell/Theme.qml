// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import Quickshell

Singleton {
    id: root

    // --- L1 Design Tokens (static) ---
    readonly property int spaceXs: 4
    readonly property int spaceSm: 8
    readonly property int spaceMd: 12
    readonly property int spaceLg: 16
    readonly property int spaceXl: 24
    readonly property int spaceXxl: 32

    readonly property int radiusXs: 4
    readonly property int radiusSm: 6
    readonly property int radiusMd: 10
    readonly property int radiusLg: 14

    readonly property int fontMicro: 10
    readonly property int fontSmall: 11
    readonly property int fontBody: 13
    readonly property int fontLabel: 15
    readonly property int fontDisplay: 20
    readonly property int fontHero: 32

    readonly property int weightRegular: 400
    readonly property int weightMedium: 500
    readonly property int weightSemibold: 600

    readonly property string fontFamily: "PP Editorial New, Tsukushi A Mincho, Smile Nerd Font Mono, sans-serif"

    readonly property int motionInstant: 0
    readonly property int motionFast: 120
    readonly property int motionNormal: 200
    readonly property int motionSlow: 320
    readonly property int motionDeliberate: 480

    readonly property int barOuterMargin: 8
    readonly property int barHeight: 32
    readonly property int barPadX: 12
    readonly property int barPadY: 6
    readonly property int barRadius: 14

    // Unified icon size for bar widgets and tray items (scales with barHeight)
    function barIconSize(scale) {
        var s = (scale !== undefined) ? scale : 1.0;
        return Math.round(barHeight * 0.5 * s);
    }
    readonly property int iconSize: barIconSize()

    readonly property int auxCollapsedWidth: 0
    readonly property int auxTriggerZone: 3
    readonly property int auxTriggerDelayMs: 200
    readonly property int auxExpandedWidth: 320

    readonly property int powerTriggerWidth: 280
    readonly property int powerTriggerHeight: 3
    readonly property int powerTriggerDelayMs: 200
    readonly property int powerCloseDelayMs: 120
    readonly property int powerDockWidth: 400
    readonly property int powerDockHeight: 120
    readonly property int powerActionSize: 64
    readonly property int powerConfirmTimeoutMs: 3000

    readonly property int rightPopupWidth: 420
    readonly property int rightPopupMaxHeight: 560
    readonly property int notificationPanelWidth: rightPopupWidth
    readonly property int notificationPanelMaxHeight: rightPopupMaxHeight
    readonly property int notificationListMaxHeight: 480
    readonly property int volumePanelWidth: rightPopupWidth
    readonly property int volumePanelMaxHeight: rightPopupMaxHeight
    readonly property int volumeStreamsMaxHeight: 260
    readonly property int bluetoothPanelWidth: rightPopupWidth
    readonly property int bluetoothPanelMaxHeight: rightPopupMaxHeight
    readonly property int networkPanelWidth: rightPopupWidth
    readonly property int networkPanelMaxHeight: rightPopupMaxHeight

    readonly property int clockPanelMaxWidth: 920
    readonly property int clockPanelMaxHeight: 440
    readonly property int clockPanelMinWidth: 760
    readonly property int clockWeatherChartHeight: 200
    readonly property int clockWeatherIconSize: 40

    // --- Dynamic color tokens (updated from daemon theme topic) ---
    property string bgCanvas: "#1a1b26"
    property string bgSurface: "#1a1b26"
    property string bgSurfaceRaised: "#16161e"

    property string fgPrimary: "#c0caf5"
    property string fgSecondary: "#a9b1d6"
    property string fgMuted: "#565f89"
    property string fgDisabled: "#495175"

    property string borderDefault: "#3b4261"
    property string borderSubtle: "#15161e"
    property string borderAccent: "#7aa2f7"

    property string accentBlue: "#7aa2f7"
    property string accentRed: "#f7768e"
    property string accentGreen: "#9ece6a"
    property string accentYellow: "#e0af68"
    property string accentPurple: "#bb9af7"
    property string accentOrange: "#ff9e64"
    property string accentTeal: "#1abc9c"
    property string accentCyan: "#7dcfff"

    property string colorSuccess: "#9ece6a"
    property string colorWarning: "#e0af68"
    property string colorError: "#f7768e"
    property string colorInfo: "#0db9d7"

    property string surfaceHover: "#1f2230"
    property string surfaceActive: "#283457"

    property real opacityPanel: 0.9
    property real opacityPopup: 0.94

    property int blurPanel: 28
    property int blurPopup: 22

    function _applySnapshot(snap) {
        var t = snap.tokens;
        if (!t) return;

        var core = t.core;
        if (core) {
            if (core.background) root.bgCanvas = core.background.canvas || root.bgCanvas;
            if (core.surface) {
                root.bgSurface = core.surface.base || root.bgSurface;
                root.bgSurfaceRaised = core.surface.raised || root.bgSurfaceRaised;
            }
            if (core.foreground) {
                root.fgPrimary   = core.foreground.primary   || root.fgPrimary;
                root.fgSecondary = core.foreground.secondary || root.fgSecondary;
                root.fgMuted     = core.foreground.muted     || root.fgMuted;
                root.fgDisabled  = core.foreground.disabled  || root.fgDisabled;
            }
            if (core.border) {
                root.borderDefault = core.border.default || root.borderDefault;
                root.borderSubtle  = core.border.subtle  || root.borderSubtle;
                root.borderAccent  = core.border.accent  || root.borderAccent;
            }
        }

        var sem = t.semantic;
        if (sem) {
            if (sem.success) root.colorSuccess = sem.success.fg || root.colorSuccess;
            if (sem.warning) root.colorWarning = sem.warning.fg || root.colorWarning;
            if (sem.error)   root.colorError   = sem.error.fg   || root.colorError;
            if (sem.info)    root.colorInfo     = sem.info.fg    || root.colorInfo;
        }

        var rt = t.runtime;
        if (rt) {
            if (rt.opacity && rt.opacity.glass) {
                var g = rt.opacity.glass;
                if (g.panel !== undefined) root.opacityPanel = g.panel;
                if (g.popup !== undefined) root.opacityPopup = g.popup;
            }
            if (rt.blur) {
                if (rt.blur.panel !== undefined) root.blurPanel = rt.blur.panel;
                if (rt.blur.popup !== undefined) root.blurPopup = rt.blur.popup;
            }
        }

        var pal = snap.palette;
        if (pal && pal.raw && pal.raw.accents) {
            var acc = pal.raw.accents;
            if (acc.blue)   root.accentBlue   = acc.blue;
            if (acc.red)    root.accentRed     = acc.red;
            if (acc.green)  root.accentGreen   = acc.green;
            if (acc.yellow) root.accentYellow  = acc.yellow;
            if (acc.purple) root.accentPurple  = acc.purple;
            if (acc.orange) root.accentOrange  = acc.orange;
            if (acc.teal)   root.accentTeal    = acc.teal;
            if (acc.cyan)   root.accentCyan    = acc.cyan;
        }
        if (pal && pal.derived) {
            root.surfaceHover  = pal.derived.surface_hover_soft   || root.surfaceHover;
            root.surfaceActive = pal.derived.selection_subtle_mix || root.surfaceActive;
        }
    }
}
