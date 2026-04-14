// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Item {
    id: root

    property string iconPath: ""
    property color  color: "transparent"
    property int    size: 16

    width:  root.size
    height: root.size

    Image {
        id: img
        anchors.fill: parent
        source: root.iconPath ? Qt.resolvedUrl(root.iconPath) : ""
        fillMode: Image.PreserveAspectFit
        visible: status !== Image.Error && root.iconPath !== ""
        smooth: true
    }

    Rectangle {
        id: placeholder
        anchors.fill: parent
        color: "transparent"
        border.color: root.color
        border.width: 1
        radius: 2
        visible: img.status === Image.Error || root.iconPath === ""
    }
}
