// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Item {
    id: root

    implicitWidth: Theme.statusCapsuleSlotWidth
    implicitHeight: Theme.statusCapsuleHeight

    signal clicked()

    visible: Network.connected || Network.ready

    readonly property string _iconPath: {
        if (Network.wiredConnected)
            return "lucide/ethernet-port.svg";
        if (Network.wifiConnected)
            return Theme.wifiIconForSignal(Network.signalPct);
        return Theme.iconWifiZeroStatus;
    }

    readonly property color _color: {
        if (!Network.ready)
            return Theme.fgMuted;
        if (Network.isDisabled || Network.isUnavailable)
            return Theme.fgMuted;
        if (Network.scanning)
            return Theme.accentBlue;
        return Theme.fgPrimary;
    }

    Item {
        id: iconWrap
        anchors.centerIn: parent
        width: Theme.statusIconSize
        height: Theme.statusIconSize

        SvgIcon {
            anchors.centerIn: parent
            iconPath: root._iconPath
            size: Theme.statusIconSize
            color: root._color
        }

        SequentialAnimation on opacity {
            running: Network.scanning && !Network.wiredConnected
            loops: Animation.Infinite

            NumberAnimation { to: 0.38; duration: 600 }
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
