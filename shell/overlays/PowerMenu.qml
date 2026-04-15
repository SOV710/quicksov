// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell.Io
import ".."
import "../components"

Rectangle {
    id: root

    signal closed()

    width:  240
    height: menuCol.implicitHeight + Theme.spaceLg * 2
    radius: Theme.radiusMd
    color:  Theme.bgSurface
    border.color: Theme.borderDefault
    border.width: 1
    opacity: visible ? Theme.opacityPopup : 0

    Behavior on opacity { NumberAnimation { duration: Theme.motionFast } }

    Column {
        id: menuCol
        anchors {
            fill: parent
            margins: Theme.spaceLg
        }
        spacing: Theme.spaceXs

        Text {
            text: "Power"
            color: Theme.fgPrimary
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontLabel
            font.weight: Theme.weightSemibold
            width: parent.width
        }

        Repeater {
            model: [
                { label: "Suspend",  iconPath: "lucide/moon.svg",      cmd: ["systemctl", "suspend"]     },
                { label: "Reboot",   iconPath: "lucide/rotate-cw.svg", cmd: ["systemctl", "reboot"]      },
                { label: "Shut down",iconPath: "phosphor/power.svg",   cmd: ["systemctl", "poweroff"]    },
                { label: "Lock",     iconPath: "lucide/lock.svg",      cmd: ["loginctl", "lock-session"]  }
            ]

            delegate: PowerItem {
                required property var modelData
                entry: modelData
                width: menuCol.width
            }
        }
    }

    component PowerItem: Rectangle {
        property var entry: null
        height: 36
        radius: Theme.radiusSm
        color: hovered ? Theme.surfaceHover : "transparent"

        property bool hovered: false

        Behavior on color { ColorAnimation { duration: Theme.motionFast } }

        Row {
            anchors { verticalCenter: parent.verticalCenter; left: parent.left; leftMargin: Theme.spaceSm }
            spacing: Theme.spaceSm

            SvgIcon {
                iconPath: entry ? entry.iconPath : "lucide/x.svg"
                size: Theme.iconSize
                color: Theme.fgSecondary
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                text: entry ? entry.label : ""
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                anchors.verticalCenter: parent.verticalCenter
            }
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onEntered: parent.hovered = true
            onExited:  parent.hovered = false
            onClicked: {
                if (entry) {
                    var p = Qt.createQmlObject('import Quickshell.Io; Process { }', root);
                    p.command = entry.cmd;
                    p.running = true;
                }
                root.closed();
            }
        }
    }
}
