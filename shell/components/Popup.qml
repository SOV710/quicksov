// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Item {
    id: root

    property bool popupVisible: false
    property int  popupWidth: 280
    property int  popupHeight: 200

    default property alias content: popupContent.data

    Rectangle {
        id: panel
        width:  root.popupWidth
        height: root.popupHeight
        radius: Theme.radiusMd
        color: Theme.bgSurface
        border.color: Theme.borderDefault
        border.width: 1
        visible: root.popupVisible
        opacity: root.popupVisible ? Theme.opacityPopup : 0

        Item {
            id: popupContent
            anchors {
                fill: parent
                margins: Theme.spaceMd
            }
        }

        Behavior on opacity { NumberAnimation { duration: Theme.motionFast } }
    }
}
