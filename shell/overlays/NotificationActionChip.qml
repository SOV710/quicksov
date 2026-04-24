// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Rectangle {
    id: root

    property color accentColor: Theme.accentBlue
    property bool emphasized: false
    property bool interactive: true
    property string label: ""

    signal clicked()

    implicitHeight: 28
    implicitWidth: chipLabel.implicitWidth + Theme.spaceMd * 2
    radius: Theme.radiusSm
    border.color: emphasized ? Theme.withAlpha(accentColor, 0.44) : Theme.borderDefault
    border.width: 1
    color: chipHover.hovered && root.interactive
           ? (emphasized
              ? Theme.overlay(Theme.surfaceActive, accentColor, 0.32)
              : Theme.surfaceHover)
           : (emphasized
              ? Theme.overlay(Theme.chromeSubtleFill, accentColor, 0.20)
              : Theme.bgSurfaceRaised)

    Text {
        id: chipLabel

        anchors.centerIn: parent
        color: emphasized ? Theme.fgPrimary : Theme.fgSecondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontBody
        font.weight: emphasized ? Theme.weightMedium : Theme.weightRegular
        text: root.label
    }

    HoverHandler {
        id: chipHover
        cursorShape: root.interactive ? Qt.PointingHandCursor : Qt.ArrowCursor
    }

    MouseArea {
        anchors.fill: parent
        enabled: root.interactive
        cursorShape: root.interactive ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: root.clicked()
    }
}
