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

    // Keep the slot visible once the daemon is connected so "off" can render
    // as an explicit bluetooth-off icon instead of disappearing entirely.
    visible: Bluetooth.connected || Bluetooth.ready

    readonly property string _iconPath: Bluetooth.btEnabled
                                        ? "lucide/bluetooth.svg"
                                        : "lucide/bluetooth-off.svg"

    readonly property color _color: {
        if (!Bluetooth.btEnabled) return Theme.fgMuted;
        if (Bluetooth.connectedDevices.length > 0) return Theme.accentBlue;
        return Theme.fgSecondary;
    }

    SvgIcon {
        id: _icon
        iconPath: root._iconPath
        size: Theme.iconSize
        color: root._color
    }
}
