// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Item {
    id: root

    property string iconPath: ""
    property int    iconSize: 14
    property string tooltipText: ""
    property bool   active: false

    signal clicked()
    signal rightClicked()

    width:  root.iconSize + Theme.spaceSm * 2
    height: root.iconSize + Theme.spaceXs * 2

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: Theme.radiusSm
        color: mouseArea.containsMouse
               ? (root.active ? Theme.surfaceActive : Theme.surfaceHover)
               : (root.active ? Theme.surfaceActive : "transparent")

        Behavior on color { ColorAnimation { duration: Theme.motionFast } }

        SvgIcon {
            anchors.centerIn: parent
            iconPath: root.iconPath
            size: root.iconSize
            color: Theme.fgMuted
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: function(mouse) {
            if (mouse.button === Qt.RightButton) root.rightClicked();
            else root.clicked();
        }
    }
}
