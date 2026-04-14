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
        height: 20

        Rectangle {
            id: dotRect
            anchors.verticalCenter: parent.verticalCenter
            width:  wsData && wsData.is_active ? 18 : 6
            height: 6
            radius: 3
            color:  wsData && wsData.is_active ? Theme.accentBlue
                  : wsData && wsData.active_window_id !== null ? Theme.fgSecondary
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
