// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import "../panels"
import "../services"

Scope {
    Variants {
        model: Quickshell.screens

        Item {
            id: root

            required property var modelData
            readonly property bool isMainScreen: Meta.ready
                                              && Meta.hasScreenRoles
                                              && Meta.screenRoles[modelData.name] === "main"

            MainBarExclusiveZoneWindow {
                screenModel: root.modelData
            }

            MainBarOverlayWindow {
                screenModel: root.modelData
            }

            Loader {
                active: root.isMainScreen && NotificationUiState.toastSurfaceActive
                sourceComponent: toastWindowComponent
            }

            Component {
                id: toastWindowComponent

                NotificationToastWindow {
                    screenModel: root.modelData
                }
            }
        }
    }
}
