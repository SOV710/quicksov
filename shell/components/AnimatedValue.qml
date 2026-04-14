// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Item {
    id: root

    property real value: 0
    property real displayValue: 0
    property int  duration: 200
    property int  easingType: Easing.OutCubic

    Behavior on displayValue {
        NumberAnimation {
            duration: root.duration
            easing.type: root.easingType
        }
    }

    onValueChanged: root.displayValue = root.value
}
