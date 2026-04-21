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
            spacing: Theme.spaceSm

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
        width: dotRect.width
        height: Theme.workspaceSpotSize

        Rectangle {
            id: dotRect
            anchors.verticalCenter: parent.verticalCenter
            width: wsData && wsData.focused ? Theme.workspaceActiveSpotWidth : Theme.workspaceSpotSize
            height: Theme.workspaceSpotSize
            radius: Theme.workspaceSpotSize / 2
            color: wsData && wsData.focused ? Theme.workspaceSpotActive
                 : wsData && wsData.windows > 0 ? Theme.workspaceSpotFilled
                 : Theme.workspaceSpotEmpty

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
