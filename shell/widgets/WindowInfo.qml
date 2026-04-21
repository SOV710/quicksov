// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property real maxWidth: 320
    readonly property real _innerMaxWidth: Math.max(0, root.maxWidth - Theme.groupContainerPadX * 2)
    readonly property real _textBudget: Math.max(
        0,
        root._innerMaxWidth - separator.implicitWidth - content.spacing * 2
    )
    readonly property real _segmentMaxWidth: Math.floor(_textBudget / 2)
    readonly property real _appWidth: Math.min(appLabel.implicitWidth, root._segmentMaxWidth)
    readonly property real _titleWidth: Math.min(titleLabel.implicitWidth, root._segmentMaxWidth)

    width: Math.min(
        root.maxWidth,
        root._appWidth + separator.implicitWidth + root._titleWidth + content.spacing * 2 + Theme.groupContainerPadX * 2
    )
    implicitWidth: root._appWidth
                 + separator.implicitWidth
                 + root._titleWidth
                 + content.spacing * 2
                 + Theme.groupContainerPadX * 2
    implicitHeight: Theme.groupContainerHeight

    readonly property var _window: (Niri.ready ? Niri.focusedWindow : null)
    readonly property bool _hasWindow: !!_window
    readonly property string _appName: {
        if (!root._hasWindow) return "No window";
        return root._window.display_name || root._window.app_id || "Unknown";
    }
    readonly property string _title: {
        if (!root._hasWindow) return "No focused window";
        return root._window.title || root._window.app_id || "Untitled";
    }

    Rectangle {
        anchors.fill: parent
        radius: Theme.groupContainerRadius
        color: Theme.groupContainerFill
        border.color: Theme.groupContainerBorder
        border.width: 1

        Row {
            id: content
            width: root.width - Theme.groupContainerPadX * 2
            height: parent.height
            anchors.centerIn: parent
            spacing: Theme.spaceXs

            Text {
                id: appLabel
                width: root._appWidth
                height: parent.height
                text: root._appName
                color: root._hasWindow ? Theme.fgPrimary : Theme.fgMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                font.weight: Theme.weightMedium
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }

            Text {
                id: separator
                height: parent.height
                text: "•"
                color: Theme.fgMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                font.weight: Theme.weightRegular
                verticalAlignment: Text.AlignVCenter
            }

            Text {
                id: titleLabel
                width: root._titleWidth
                height: parent.height
                text: root._title
                color: root._hasWindow ? Theme.fgSecondary : Theme.fgMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                font.weight: Theme.weightRegular
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
        }
    }
}
