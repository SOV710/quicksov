// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Item {
    id: root

    implicitWidth: _icon.implicitWidth
    implicitHeight: _icon.implicitHeight

    readonly property bool _connected: {
        if (Network.wifiConnected) return true;
        var ifaces = Network.interfaces;
        for (var i = 0; i < ifaces.length; i++) {
            if (ifaces[i].up && ifaces[i].carrier) return true;
        }
        return false;
    }

    readonly property int _signalPct: Network.signalPct

    readonly property string _iconPath: {
        if (Network.wifiConnected) {
            if (root._signalPct < 0) return "lucide/wifi.svg";
            if (root._signalPct < 25) return "lucide/wifi-zero.svg";
            if (root._signalPct < 50) return "lucide/wifi-low.svg";
            if (root._signalPct < 75) return "lucide/wifi.svg";
            return "lucide/wifi-high.svg";
        }
        if (!root._connected) return "lucide/wifi-off.svg";
        return "lucide/wifi-high.svg";
    }

    SvgIcon {
        id: _icon
        iconPath: root._iconPath
        size: Theme.iconSize
        color: root._connected ? Theme.fgPrimary : Theme.fgMuted
    }
}
