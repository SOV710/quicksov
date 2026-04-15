// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    implicitWidth: label.implicitWidth
    implicitHeight: label.implicitHeight

    visible: Bluetooth.btAvailable

    property color _color: {
        if (!Bluetooth.btEnabled) return Theme.fgMuted;
        if (Bluetooth.connectedDevices.length > 0) return Theme.accentBlue;
        return Theme.fgSecondary;
    }

    Text {
        id: label
        text: "󰂯"
        color: root._color
        font.pixelSize: Theme.iconSize
        font.family: Theme.fontFamily
    }
}
