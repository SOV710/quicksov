// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Item {
    id: root

    signal clicked()

    implicitWidth: Theme.statusCapsuleSlotWidth
    implicitHeight: Theme.statusCapsuleHeight
    visible: Battery.ready && Battery.present

    readonly property color _color: {
        if (Battery.chargeStatus === "charging" || Battery.chargeStatus === "fully_charged")
            return Theme.colorSuccess;
        return Theme.fgPrimary;
    }

    SvgIcon {
        anchors.centerIn: parent
        iconPath: Theme.batteryIconForLevel(Battery.percentage, Battery.chargeStatus)
        size: Theme.iconSize
        color: root._color
    }

    MouseArea {
        anchors.fill: parent
        enabled: Battery.ready
        hoverEnabled: enabled
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: root.clicked()
    }
}
