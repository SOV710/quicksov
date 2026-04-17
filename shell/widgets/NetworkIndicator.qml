// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Item {
    id: root

    implicitWidth: iconWrap.implicitWidth
    implicitHeight: iconWrap.implicitHeight

    signal clicked()

    visible: Network.connected || Network.ready

    readonly property string _iconPath: {
        if (!Network.ready)
            return "lucide/wifi.svg";
        if (Network.wifiConnected)
            return Network.iconPathForSignal(Network.signalPct);
        if (Network.wiredConnected)
            return "lucide/ethernet-port.svg";
        if (Network.isDisabled || Network.isUnavailable)
            return "lucide/wifi-off.svg";
        return "lucide/wifi.svg";
    }

    readonly property color _color: {
        if (!Network.ready)
            return Theme.fgMuted;
        if (Network.wifiConnected)
            return Theme.accentBlue;
        if (Network.wiredConnected)
            return Theme.fgPrimary;
        if (Network.isDisabled || Network.isUnavailable)
            return Theme.fgMuted;
        if (Network.scanning)
            return Theme.accentBlue;
        return Theme.fgSecondary;
    }

    Item {
        id: iconWrap

        implicitWidth: icon.implicitWidth
        implicitHeight: icon.implicitHeight

        SvgIcon {
            id: icon
            iconPath: root._iconPath
            size: Theme.iconSize
            color: root._color
        }

        SequentialAnimation on opacity {
            id: scanPulse
            running: Network.scanning && !Network.wiredConnected
            loops: Animation.Infinite

            NumberAnimation { to: 0.4; duration: 600 }
            NumberAnimation { to: 1.0; duration: 600 }

            onRunningChanged: {
                if (!running)
                    iconWrap.opacity = 1.0;
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
}
