// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import ".."
import "../services"
import "../overlays"

Scope {
    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: bar

            required property var modelData
            screen: modelData
            // Show on any screen that is not the designated main screen.
            visible: Meta.ready && Meta.hasScreenRoles && Meta.screenRoles[modelData.name] !== "main"

            anchors.left: true
            anchors.top:  true
            anchors.bottom: true

            margins {
                left:   0
                top:    Theme.barOuterMargin
                bottom: Theme.barOuterMargin
            }

            implicitWidth: triggerZone.width
            color: "transparent"

            property bool expanded: false

            // Invisible trigger zone
            Rectangle {
                id: triggerZone
                width:  Theme.auxTriggerZone
                height: parent.height
                color:  "transparent"

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    onEntered: expandTimer.start()
                    onExited:  expandTimer.stop()
                }
            }

            Timer {
                id: expandTimer
                interval: Theme.auxTriggerDelayMs
                onTriggered: bar.expanded = true
            }

            MusicPanel {
                id: musicPanel
                visible: bar.expanded
                width:   Theme.auxExpandedWidth
                height:  parent.height - Theme.barOuterMargin * 2
                y:       Theme.barOuterMargin

                onCloseRequested: bar.expanded = false
            }

            Behavior on implicitWidth {
                NumberAnimation { duration: Theme.motionNormal; easing.type: Easing.OutCubic }
            }
        }
    }
}
