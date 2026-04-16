// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import org.kde.kirigami.primitives as KirigamiPrimitives

// Renders a monochrome SVG icon (lucide/phosphor) tinted to `color`.
// iconPath: relative to the icons/ root, e.g. "lucide/wifi.svg"
KirigamiPrimitives.Icon {
    id: root

    property string iconPath: ""
    property int size: 16

    implicitWidth:  root.size
    implicitHeight: root.size
    width:  root.size
    height: root.size
    source: root.iconPath ? Qt.resolvedUrl("../icons/" + root.iconPath) : ""
    isMask: true
    smooth: true
}
