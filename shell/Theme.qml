// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import Quickshell
import "./ipc"

Singleton {
    id: root

    property bool themeConnected: false
    property bool themeReady: false
    property string themeStatus: "disconnected"
    property string themeLastError: ""

    // --- L1 Design Tokens (static) ---
    readonly property int spaceXs: 4
    readonly property int spaceSm: 8
    readonly property int spaceMd: 12
    readonly property int spaceLg: 16
    readonly property int spaceXl: 24
    readonly property int spaceXxl: 32

    readonly property int radiusXs: 8
    readonly property int radiusSm: 12
    readonly property int radiusMd: 16
    readonly property int radiusLg: 20
    readonly property int radiusXl: 28

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

    readonly property int barOuterMargin: 20
    readonly property int barHeight: 32
    readonly property int barPadX: 16
    readonly property int barPadY: 0
    readonly property int barRadius: 20
    readonly property int popupGap: 12
    readonly property int panelEdgeInset: 24
    readonly property int statusPopupRightInset: 16
    readonly property int groupContainerHeight: 24
    readonly property int groupContainerPadX: 8
    readonly property int groupContainerRadius: 16
    readonly property int leafChipHeight: 20
    readonly property int leafChipRadius: 12
    readonly property int statusCapsuleHeight: 26
    readonly property int statusCapsuleRadius: 13
    readonly property int statusCapsulePadX: 8
    readonly property int statusCapsuleSlotWidth: 24
    readonly property int statusDockShoulderDepth: 18
    readonly property int statusDockLowerRadius: 28
    readonly property int statusDockRevealDuration: 150
    readonly property int trayChipHeight: 24
    readonly property int trayChipPad: 4
    readonly property int trayChipRadius: 12
    readonly property int clockSegmentHeight: 24
    readonly property int clockSegmentRadius: 12
    readonly property int clockCapsulePadX: 10
    readonly property int clockCapsuleGap: 8
    readonly property int workspaceSpotSize: 12
    readonly property real workspaceGooeyBlobScale: 1.36
    readonly property int workspaceGooeyDuration: 320
    readonly property real workspaceGooeyMergeStrength: 7.0

    // Unified icon size for bar widgets and tray items (scales with barHeight)
    function barIconSize(scale) {
        var s = (scale !== undefined) ? scale : 1.0;
        return Math.round(barHeight * 0.44 * s);
    }
    readonly property int iconSize: barIconSize()
    readonly property int statusIconSize: iconSize + 2

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

    readonly property int rightPopupWidth: 440
    readonly property int rightPopupMaxHeight: 380
    readonly property int notificationPanelWidth: rightPopupWidth
    readonly property int notificationPanelMaxHeight: rightPopupMaxHeight
    readonly property int notificationListMaxHeight: 300
    readonly property int volumePanelWidth: rightPopupWidth
    readonly property int volumePanelMaxHeight: rightPopupMaxHeight
    readonly property int volumeStreamsMaxHeight: 220
    readonly property int bluetoothPanelWidth: rightPopupWidth
    readonly property int bluetoothPanelMaxHeight: rightPopupMaxHeight
    readonly property int networkPanelWidth: rightPopupWidth
    readonly property int networkPanelMaxHeight: rightPopupMaxHeight
    readonly property int batteryPanelWidth: rightPopupWidth
    readonly property int batteryPanelMaxHeight: rightPopupMaxHeight

    readonly property int clockPanelMaxWidth: 1040
    readonly property int clockPanelMaxHeight: 520
    readonly property int clockPanelMinWidth: 920
    readonly property int clockWeatherChartHeight: 220
    readonly property int clockWeatherIconSize: 40

    // --- Dynamic color tokens (updated from daemon theme topic) ---
    property color bgCanvas: "#1a1b26"
    property color bgSurface: "#1a1b26"
    property color bgSurfaceRaised: "#16161e"

    property color fgPrimary: "#c0caf5"
    property color fgSecondary: "#a9b1d6"
    property color fgMuted: "#565f89"
    property color fgDisabled: "#495175"

    property color borderDefault: "#3b4261"
    property color borderSubtle: "#15161e"
    property color borderAccent: "#7aa2f7"

    property color accentBlue: "#7aa2f7"
    property color accentRed: "#f7768e"
    property color accentGreen: "#9ece6a"
    property color accentYellow: "#e0af68"
    property color accentPurple: "#9d7cd8"
    property color accentOrange: "#ff9e64"
    property color accentTeal: "#1abc9c"
    property color accentCyan: "#7dcfff"

    property color colorSuccess: "#9ece6a"
    property color colorWarning: "#e0af68"
    property color colorError: "#db4b4b"
    property color colorInfo: "#0db9d7"

    property color surfaceHover: "#1f2230"
    property color surfaceActive: "#283250"
    property color overlayScrim: "#11121a"
    property color overlayScrimStrong: "#13151c"
    property color shadowBase: "#0C0E14"
    property color shadowAlt: "#15161e"

    property real opacityPanel: 0.9
    property real opacityPopup: 0.94

    property int blurPanel: 28
    property int blurPopup: 22

    function normalizeColor(value) {
        if (value && value.r !== undefined && value.g !== undefined && value.b !== undefined)
            return value;
        return Qt.color(String(value));
    }

    function withAlpha(color, alpha) {
        var c = normalizeColor(color);
        return Qt.rgba(c.r, c.g, c.b, alpha);
    }

    function overlay(base, tint, alpha) {
        return Qt.tint(normalizeColor(base), withAlpha(tint, alpha));
    }

    readonly property color barShadowColor: withAlpha(shadowBase, 0.09)
    readonly property color panelShadowColor: withAlpha(shadowBase, 0.22)
    readonly property color chromeSubtleFill: overlay(bgSurface, fgPrimary, 0.04)
    readonly property color chromeSubtleFillMuted: overlay(bgSurface, fgPrimary, 0.03)
    readonly property color hitAreaRevealFill: overlay(bgSurface, fgPrimary, 0.01)
    readonly property color dangerBorderSoft: withAlpha(colorError, 0.50)
    readonly property color barShellFill: withAlpha(overlay(bgSurface, fgSecondary, 0.20), 0.82)
    readonly property color barShellBorder: withAlpha(borderDefault, 0.24)
    readonly property color popupShellFill: withAlpha(overlay(bgSurface, fgPrimary, 0.08), 0.88)
    readonly property color popupShellBorder: withAlpha(borderDefault, 0.30)
    readonly property color groupContainerFill: overlay(barShellFill, bgSurfaceRaised, 0.40)
    readonly property color groupContainerBorder: withAlpha(borderDefault, 0.14)
    readonly property color workspaceContainerFill: overlay(barShellFill, accentTeal, 0.20)
    readonly property color workspaceContainerBorder: withAlpha(accentTeal, 0.24)
    readonly property color workspaceSpotActive: withAlpha(accentTeal, 0.96)
    readonly property color workspaceSpotFilled: withAlpha(accentTeal, 0.60)
    readonly property color workspaceSpotEmpty: withAlpha(accentTeal, 0.40)
    readonly property color trayChipFill: overlay(barShellFill, fgSecondary, 0.06)
    readonly property color trayChipHover: overlay(barShellFill, accentBlue, 0.10)
    readonly property color trayChipBorder: withAlpha(borderDefault, 0.14)
    readonly property color clockCapsuleFill: overlay(barShellFill, bgSurface, 0.90)
    readonly property color clockCapsuleText: fgPrimary
    readonly property color statusCapsuleFill: overlay(barShellFill, accentTeal, 0.24)
    readonly property color statusCapsuleBorder: withAlpha(accentTeal, 0.22)
    readonly property color statusDockFill: barShellFill
    readonly property color statusDockBorder: barShellBorder

    readonly property string iconBatteryStatus: "material/battery_android_6_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconBatteryFullStatus: "material/battery_android_full_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconWifiStatus: "material/network_wifi_3_bar_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconWifiZeroStatus: "material/signal_wifi_0_bar_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconWifiOneStatus: "material/network_wifi_1_bar_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconWifiTwoStatus: "material/network_wifi_2_bar_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconWifiThreeStatus: "material/network_wifi_3_bar_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconWifiFourStatus: "material/signal_wifi_4_bar_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconBluetoothStatus: "material/bluetooth_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconBluetoothOffStatus: "material/bluetooth_disabled_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconVolumeStatus: "material/volume_up_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"
    readonly property string iconNotificationStatus: "material/notifications_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg"

    function batteryIconForLevel(level, chargeStatus) {
        if (chargeStatus === "fully_charged" || level >= 0.99)
            return iconBatteryFullStatus;
        if (typeof level !== "number")
            return iconBatteryStatus;
        if (level <= 0.08)
            return "material/battery_android_0_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg";
        if (level <= 0.22)
            return "material/battery_android_1_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg";
        if (level <= 0.35)
            return "material/battery_android_2_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg";
        if (level <= 0.50)
            return "material/battery_android_3_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg";
        if (level <= 0.65)
            return "material/battery_android_4_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg";
        if (level <= 0.82)
            return "material/battery_android_5_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg";
        return iconBatteryStatus;
    }

    function wifiIconForSignal(signalPct) {
        if (signalPct < 0)
            return iconWifiZeroStatus;
        if (signalPct < 20)
            return iconWifiZeroStatus;
        if (signalPct < 40)
            return iconWifiOneStatus;
        if (signalPct < 60)
            return iconWifiTwoStatus;
        if (signalPct < 80)
            return iconWifiThreeStatus;
        return iconWifiFourStatus;
    }

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
            if (core.overlay) {
                root.overlayScrim = core.overlay.scrim || root.overlayScrim;
                root.overlayScrimStrong = core.overlay.scrim_strong || root.overlayScrimStrong;
                root.shadowBase = core.overlay.shadow || root.shadowBase;
                root.shadowAlt = core.overlay.shadow_alt || root.shadowAlt;
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

        root.themeReady = true;
        root.themeStatus = "ok";
    }

    function _onThemeConnectionChanged(isConnected) {
        root.themeConnected = isConnected;
        if (isConnected) {
            Client.subscribe("theme", root._applySnapshot);
        } else {
            root.themeReady = false;
            root.themeStatus = "disconnected";
        }
    }

    Component.onCompleted: {
        Client.connectionChanged.connect(root._onThemeConnectionChanged);
        if (Client.connected)
            root._onThemeConnectionChanged(true);
    }
}
