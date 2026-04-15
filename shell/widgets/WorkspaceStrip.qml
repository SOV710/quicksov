// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property string outputName: ""

    implicitWidth: row.implicitWidth + Theme.spaceXs * 2
    implicitHeight: row.implicitHeight

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

    component WorkspaceDot: Item {
        property var wsData: null
        width: dotRect.width + Theme.spaceXs
        height: 24

        Rectangle {
            id: dotRect
            anchors.verticalCenter: parent.verticalCenter
            // Daemon sends "focused" (bool), not "is_active"
            width:  wsData && wsData.focused ? 22 : 8
            height: 8
            radius: 4
            // Daemon sends "windows" (count), not "active_window_id"
            color:  wsData && wsData.focused ? Theme.accentBlue
                  : wsData && wsData.windows > 0 ? Theme.fgSecondary
                  : Theme.fgMuted

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
