// SPDX-FileCopyrightText: 2026 SOV710
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

    SvgIcon {
        id: _icon
        iconPath: root._connected ? "lucide/wifi.svg" : "lucide/wifi-off.svg"
        size: Theme.iconSize
        color: root._connected ? Theme.fgPrimary : Theme.fgMuted
    }
}
