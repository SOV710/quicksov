// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import "../panels"

Scope {
    Variants {
        model: Quickshell.screens

        Item {
            id: root

            required property var modelData

            MainBarExclusiveZoneWindow {
                screenModel: root.modelData
            }

            MainBarOverlayWindow {
                screenModel: root.modelData
            }

            NotificationToastWindow {
                screenModel: root.modelData
            }
        }
    }
}
