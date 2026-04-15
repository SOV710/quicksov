// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    implicitWidth: label.implicitWidth + Theme.spaceXs
    implicitHeight: label.implicitHeight

    property string _icon: {
        if (!Network.ready) return "󰤯";
        if (Network.wifiConnected) {
            var dbm = Network.signalDbm;
            if (dbm >= -55) return "󰤨";
            if (dbm >= -70) return "󰤥";
            if (dbm >= -85) return "󰤢";
            return "󰤟";
        }
        var ifaces = Network.interfaces;
        for (var i = 0; i < ifaces.length; i++) {
            if (ifaces[i].up && ifaces[i].carrier) return "󰈀";
        }
        return "󰤮";
    }

    property color _color: {
        if (!Network.ready) return Theme.fgMuted;
        return (Network.wifiConnected || Network.interfaces.some(function(i) { return i.up && i.carrier; }))
               ? Theme.fgPrimary : Theme.fgMuted;
    }

    Text {
        id: label
        text: root._icon
        color: root._color
        font.pixelSize: Theme.fontLabel
        font.family: Theme.fontFamily
    }
}
