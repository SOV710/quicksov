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

    implicitWidth: row.implicitWidth
    implicitHeight: row.implicitHeight

    readonly property string _icon: {
        if (!Battery.present) return "";
        if (Battery.chargeStatus === "charging")
            return "lucide/battery-charging.svg";
        if (!Battery.onBattery && Battery.chargeStatus === "fully_charged")
            return "lucide/battery-full.svg";
        var pct = Battery.percentage;
        if (pct > 70) return "lucide/battery-full.svg";
        if (pct > 30) return "lucide/battery-medium.svg";
        if (pct > 15) return "lucide/battery-low.svg";
        return "lucide/battery-warning.svg";
    }

    readonly property color _color: {
        if (Battery.chargeStatus === "charging" || Battery.chargeStatus === "fully_charged")
            return Theme.colorSuccess;
        var pct = Battery.percentage;
        if (pct <= 15) return Theme.colorError;
        if (pct <= 30) return Theme.colorWarning;
        return Theme.fgPrimary;
    }

    Row {
        id: row
        spacing: Theme.spaceXs
        anchors.verticalCenter: parent.verticalCenter

        SvgIcon {
            iconPath: root._icon
            size: Theme.iconSize
            color: root._color
            visible: Battery.present
            anchors.verticalCenter: parent.verticalCenter
        }

        Text {
            text: Battery.present ? Math.round(Battery.percentage) + "%" : "—"
            color: root._color
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Theme.weightRegular
            font.features: { "tnum": 1 }
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        anchors.fill: parent
        enabled: Battery.ready
        hoverEnabled: enabled
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: root.clicked()
    }
}
