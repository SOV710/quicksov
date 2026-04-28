// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
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
        width:  chip.width
        height: chip.height

        readonly property bool _hovered: hoverHandler.hovered

        function openMenu() {
            if (!trayItem || !trayItem.hasMenu || !trayItem.menu)
                return;

            if (menuAnchor.visible) {
                menuAnchor.close();
                return;
            }

            menuAnchor.anchor.rect = root.QsWindow.itemRect(chip);
            menuAnchor.open();
        }

        QsMenuAnchor {
            id: menuAnchor
            menu: trayItem && trayItem.hasMenu ? trayItem.menu : null

            anchor {
                window: root.QsWindow.window
                edges: Edges.Bottom | Edges.Right
                gravity: Edges.Bottom | Edges.Left
                adjustment: PopupAdjustment.All
            }
        }

        Rectangle {
            id: chip
            width: Math.max(Theme.trayChipHeight, Theme.iconSize + Theme.trayChipPad * 2)
            height: Theme.trayChipHeight
            radius: Theme.trayChipRadius
            color: parent._hovered ? Theme.trayChipHover : Theme.trayChipFill
            border.color: Theme.trayChipBorder
            border.width: 1

            Image {
                id: icon
                anchors.centerIn: parent
                width: Theme.iconSize
                height: Theme.iconSize
                source: trayItem && trayItem.icon ? trayItem.icon : ""
                fillMode: Image.PreserveAspectFit
                visible: status !== Image.Error
                smooth: true
            }

            Rectangle {
                anchors.centerIn: parent
                width: Theme.iconSize
                height: Theme.iconSize
                color: "transparent"
                border.color: Theme.fgMuted
                border.width: 1
                radius: Theme.radiusXs
                visible: icon.status === Image.Error
            }
        }

        HoverHandler { id: hoverHandler }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: Qt.PointingHandCursor
            onClicked: function(mouse) {
                if (!trayItem) return;
                if (mouse.button === Qt.RightButton) {
                    openMenu();
                } else {
                    if (!trayItem.onlyMenu) trayItem.activate();
                    else openMenu();
                }
            }
        }
    }
}
