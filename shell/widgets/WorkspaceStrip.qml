// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property string outputName: ""

    implicitWidth: container.implicitWidth
    implicitHeight: container.implicitHeight

    Rectangle {
        id: container
        implicitWidth: row.implicitWidth + Theme.groupContainerPadX * 2
        implicitHeight: Theme.groupContainerHeight
        width: implicitWidth
        height: implicitHeight
        radius: Theme.groupContainerRadius
        color: Theme.workspaceContainerFill
        border.color: Theme.workspaceContainerBorder
        border.width: 1

        Row {
            id: row
            anchors.centerIn: parent
            spacing: Theme.spaceXs

            Repeater {
                model: Niri.ready ? Niri.workspacesForOutput(root.outputName) : []
                delegate: WorkspaceDot {
                    required property var modelData
                    wsData: modelData
                }
            }
        }
    }

    component WorkspaceDot: Item {
        property var wsData: null
        width: dotRect.width + Theme.spaceXs * 2
        height: Theme.leafChipHeight

        Rectangle {
            id: dotRect
            anchors.verticalCenter: parent.verticalCenter
            width: wsData && wsData.focused ? 28 : 10
            height: 10
            radius: 5
            color: wsData && wsData.focused ? Theme.accentTeal
                 : wsData && wsData.windows > 0 ? Theme.withAlpha(Theme.accentTeal, 0.54)
                 : Theme.withAlpha(Theme.accentTeal, 0.22)

            Behavior on width { NumberAnimation { duration: Theme.motionFast; easing.type: Easing.OutCubic } }
            Behavior on color { ColorAnimation { duration: Theme.motionFast } }
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                if (wsData) Niri.focusWorkspace(wsData.idx);
            }
        }
    }
}
