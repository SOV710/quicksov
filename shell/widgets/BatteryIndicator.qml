// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    implicitWidth: row.implicitWidth
    implicitHeight: row.implicitHeight

    property string _icon: {
        if (!Battery.present) return "";
        var pct = Battery.percentage;
        if (pct > 90) return "󰁹";
        if (pct > 70) return "󰂀";
        if (pct > 50) return "󰁾";
        if (pct > 30) return "󰁼";
        if (pct > 15) return "󰁺";
        return "󰂎";
    }

    property color _color: {
        var pct = Battery.percentage;
        if (Battery.chargeStatus === "charging") return Theme.colorSuccess;
        if (pct <= 15) return Theme.colorError;
        if (pct <= 30) return Theme.colorWarning;
        return Theme.fgPrimary;
    }

    Row {
        id: row
        spacing: Theme.spaceXs
        anchors.verticalCenter: parent.verticalCenter

        Text {
            text: root._icon
            color: root._color
            font.pixelSize: Theme.fontLabel
            font.family: Theme.fontFamily
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
}
