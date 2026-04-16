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

        Item {
            id: root

            required property var modelData
            readonly property bool isAuxScreen: Meta.ready
                                             && Meta.hasScreenRoles
                                             && Meta.screenRoles[modelData.name] !== "main"
            property bool expanded: false
            readonly property bool hoveringTrigger: triggerMouse.containsMouse
            readonly property bool hoveringPanel: panelMouse.containsMouse

            function syncHoverState() {
                if (root.hoveringTrigger) {
                    closeTimer.stop();
                    if (!root.expanded) openTimer.restart();
                    return;
                }

                openTimer.stop();
                if (root.expanded && !root.hoveringPanel) closeTimer.restart();
                else closeTimer.stop();
            }

            onHoveringTriggerChanged: root.syncHoverState()
            onHoveringPanelChanged: root.syncHoverState()
            onExpandedChanged: root.syncHoverState()

            Timer {
                id: openTimer
                interval: Theme.auxTriggerDelayMs
                onTriggered: root.expanded = true
            }

            Timer {
                id: closeTimer
                interval: Theme.powerCloseDelayMs
                onTriggered: root.expanded = false
            }

            PanelWindow {
                id: triggerWindow

                screen: root.modelData
                visible: root.isAuxScreen

                anchors.left: true
                anchors.top: true
                anchors.bottom: true

                margins {
                    left: 0
                    top: Theme.barOuterMargin
                    bottom: Theme.barOuterMargin
                }

                implicitWidth: Theme.auxTriggerZone
                color: "transparent"
                exclusiveZone: 0

                Rectangle {
                    anchors.fill: parent
                    color: Qt.rgba(1, 1, 1, 0.01)

                    MouseArea {
                        id: triggerMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.NoButton
                    }
                }
            }

            PanelWindow {
                id: panelWindow

                screen: root.modelData
                visible: root.isAuxScreen && (root.expanded || musicPanel.opacity > 0)

                anchors.left: true
                anchors.top: true
                anchors.bottom: true

                margins {
                    left: 0
                    top: Theme.barOuterMargin
                    bottom: Theme.barOuterMargin
                }

                implicitWidth: Theme.auxExpandedWidth
                color: "transparent"
                exclusiveZone: 0

                MusicPanel {
                    id: musicPanel
                    width: Theme.auxExpandedWidth
                    height: parent.height - Theme.barOuterMargin * 2
                    y: Theme.barOuterMargin
                    x: root.expanded ? 0 : -width
                    opacity: root.expanded ? Theme.opacityPanel : 0

                    onCloseRequested: root.expanded = false

                    Behavior on x {
                        NumberAnimation {
                            duration: root.expanded ? Theme.motionSlow : Theme.motionNormal
                            easing.type: root.expanded ? Easing.OutCubic : Easing.InCubic
                        }
                    }

                    Behavior on opacity {
                        NumberAnimation {
                            duration: root.expanded ? Theme.motionNormal : Theme.motionFast
                            easing.type: root.expanded ? Easing.OutCubic : Easing.InCubic
                        }
                    }

                    MouseArea {
                        id: panelMouse
                        anchors.fill: parent
                        acceptedButtons: Qt.NoButton
                        hoverEnabled: true
                    }
                }
            }
        }
    }
}
