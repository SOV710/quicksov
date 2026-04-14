// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell.Services.SystemTray
import ".."

Item {
    id: root

    implicitWidth: trayRow.implicitWidth
    implicitHeight: trayRow.implicitHeight

    Row {
        id: trayRow
        spacing: Theme.spaceXs
        anchors.verticalCenter: parent.verticalCenter

        Repeater {
            model: SystemTray.items
            delegate: TrayItem {
                required property var modelData
                trayItem: modelData
            }
        }
    }

    component TrayItem: Item {
        property var trayItem: null
        width:  20
        height: 20

        Image {
            id: icon
            anchors.fill: parent
            source: trayItem && trayItem.icon ? trayItem.icon : ""
            fillMode: Image.PreserveAspectFit
            visible: status !== Image.Error
            smooth: true
        }

        Rectangle {
            anchors.fill: parent
            color: "transparent"
            border.color: Theme.fgMuted
            border.width: 1
            radius: 2
            visible: icon.status === Image.Error
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: function(mouse) {
                if (!trayItem) return;
                if (mouse.button === Qt.RightButton) {
                    if (trayItem.menu) trayItem.menu.display(Window.window, mouseX, mouseY);
                } else {
                    if (!trayItem.onlyMenu) trayItem.activate(mouseX, mouseY);
                    else if (trayItem.menu) trayItem.menu.display(Window.window, mouseX, mouseY);
                }
            }
        }
    }
}
