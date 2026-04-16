// SPDX-FileCopyrightText: 2026 SOV710
//
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

        Item {
            id: root

            required property var modelData
            readonly property bool isMainScreen: Meta.ready
                                              && Meta.hasScreenRoles
                                              && Meta.screenRoles[modelData.name] === "main"
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
                if (root.hoveringTrigger) {
                    closeTimer.stop();
                    if (!root.expanded) openTimer.restart();
                    return;
                }

                openTimer.stop();
                if (root.expanded && !root.hoveringMenu) closeTimer.restart();
                else closeTimer.stop();
            }

            function _runAction(actionId) {
                var enabled = Meta.powerActions[actionId];
                if (enabled !== undefined && !enabled) return;

                var command = root._commandFor(actionId);
                if (!command || !command.length) return;

                actionProcess.running = false;
                actionProcess.command = command;
                actionProcess.running = true;
                root.expanded = false;
            }

            onHoveringTriggerChanged: root._syncHoverState()
            onHoveringMenuChanged: root._syncHoverState()
            onExpandedChanged: {
                root._syncHoverState();
                if (root.expanded) focusScope.forceActiveFocus();
            }

            Timer {
                id: openTimer
                interval: Theme.powerTriggerDelayMs
                onTriggered: root.expanded = true
            }

            Timer {
                id: closeTimer
                interval: Theme.powerCloseDelayMs
                onTriggered: root.expanded = false
            }

            Process {
                id: actionProcess
            }

            PanelWindow {
                id: triggerWindow

                screen: root.modelData
                visible: root.isMainScreen

                anchors.left: true
                anchors.right: true
                anchors.bottom: true

                color: "transparent"
                exclusiveZone: 0
                implicitHeight: Theme.powerTriggerHeight
                mask: Region {
                    item: triggerZone
                }

                Item {
                    anchors.fill: parent

                    Item {
                        id: triggerZone
                        width: Theme.powerTriggerWidth
                        height: Theme.powerTriggerHeight
                        anchors.horizontalCenter: parent.horizontalCenter
                        anchors.bottom: parent.bottom
                    }

                    Rectangle {
                        anchors.fill: triggerZone
                        color: Qt.rgba(1, 1, 1, 0.01)
                    }

                    MouseArea {
                        id: triggerMouse
                        anchors.fill: triggerZone
                        acceptedButtons: Qt.NoButton
                        hoverEnabled: true
                    }
                }
            }

            PanelWindow {
                id: menuWindow

                screen: root.modelData
                visible: root.isMainScreen && (root.expanded || powerMenu.opacity > 0)

                anchors.left: true
                anchors.right: true
                anchors.bottom: true

                color: "transparent"
                exclusiveZone: 0
                implicitHeight: Theme.powerDockHeight
                mask: Region {
                    item: powerMenu
                }

                FocusScope {
                    id: focusScope
                    anchors.fill: parent
                    focus: root.expanded

                    Keys.onEscapePressed: function(event) {
                        if (!root.expanded) return;
                        root.expanded = false;
                        event.accepted = true;
                    }

                    PowerMenu {
                        id: powerMenu
                        width: Theme.powerDockWidth
                        height: parent.height
                        anchors.horizontalCenter: parent.horizontalCenter
                        anchors.bottom: parent.bottom
                        opacity: root.expanded ? Theme.opacityPopup : 0
                        visible: root.expanded || opacity > 0
                        y: root.expanded ? 0 : height

                        onActionRequested: actionId => root._runAction(actionId)
                        onCloseRequested: root.expanded = false

                        Behavior on y {
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
}
