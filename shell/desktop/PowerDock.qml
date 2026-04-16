// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import Quickshell.Io
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
            visible: Meta.ready && Meta.hasScreenRoles && Meta.screenRoles[modelData.name] === "main"

            anchors.left: true
            anchors.right: true
            anchors.bottom: true

            color: "transparent"
            exclusiveZone: 0
            implicitHeight: Theme.powerDockHeight + Theme.powerTriggerHeight

            property bool expanded: false
            readonly property bool hoveringTrigger: triggerMouse.containsMouse
            readonly property bool hoveringMenu: menuMouse.containsMouse

            function _commandFor(actionId) {
                switch (actionId) {
                case "lock":
                    return ["loginctl", "lock-session"];
                case "suspend":
                    return ["loginctl", "suspend"];
                case "logout":
                    return ["niri", "msg", "action", "quit"];
                case "reboot":
                    return ["loginctl", "reboot"];
                case "shutdown":
                    return ["loginctl", "poweroff"];
                default:
                    return [];
                }
            }

            function _syncHoverState() {
                if (bar.hoveringTrigger) {
                    closeTimer.stop();
                    if (!bar.expanded) openTimer.restart();
                    return;
                }

                openTimer.stop();
                if (bar.expanded && !bar.hoveringMenu) closeTimer.restart();
                else closeTimer.stop();
            }

            function _runAction(actionId) {
                var command = bar._commandFor(actionId);
                if (!command || !command.length) return;

                actionProcess.running = false;
                actionProcess.command = command;
                actionProcess.running = true;
                bar.expanded = false;
            }

            onHoveringTriggerChanged: bar._syncHoverState()
            onHoveringMenuChanged: bar._syncHoverState()
            onExpandedChanged: {
                bar._syncHoverState();
                if (bar.expanded) focusScope.forceActiveFocus();
            }

            Timer {
                id: openTimer
                interval: Theme.powerTriggerDelayMs
                onTriggered: bar.expanded = true
            }

            Timer {
                id: closeTimer
                interval: Theme.powerCloseDelayMs
                onTriggered: bar.expanded = false
            }

            Process {
                id: actionProcess
            }

            FocusScope {
                id: focusScope
                anchors.fill: parent
                focus: bar.expanded

                Keys.onEscapePressed: function(event) {
                    if (!bar.expanded) return;
                    bar.expanded = false;
                    event.accepted = true;
                }

                Item {
                    id: triggerZone
                    width: Theme.powerTriggerWidth
                    height: Theme.powerTriggerHeight
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom

                    MouseArea {
                        id: triggerMouse
                        anchors.fill: parent
                        acceptedButtons: Qt.NoButton
                        hoverEnabled: true
                    }
                }

                PowerMenu {
                    id: powerMenu
                    width: Theme.powerDockWidth
                    height: Theme.powerDockHeight
                    x: Math.floor((focusScope.width - width) / 2)
                    y: bar.expanded
                       ? focusScope.height - Theme.powerTriggerHeight - Theme.powerDockHeight
                       : focusScope.height
                    opacity: bar.expanded ? Theme.opacityPopup : 0
                    visible: bar.expanded || opacity > 0

                    onActionRequested: actionId => bar._runAction(actionId)
                    onCloseRequested: bar.expanded = false

                    Behavior on y {
                        NumberAnimation {
                            duration: bar.expanded ? Theme.motionSlow : Theme.motionNormal
                            easing.type: bar.expanded ? Easing.OutCubic : Easing.InCubic
                        }
                    }

                    Behavior on opacity {
                        NumberAnimation {
                            duration: bar.expanded ? Theme.motionNormal : Theme.motionFast
                            easing.type: bar.expanded ? Easing.OutCubic : Easing.InCubic
                        }
                    }

                    MouseArea {
                        id: menuMouse
                        anchors.fill: parent
                        acceptedButtons: Qt.NoButton
                        hoverEnabled: true
                    }
                }
            }
        }
    }
}
