// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property real maxWidth: 320

    width: Math.min(implicitWidth, maxWidth)
    implicitWidth: appLabel.implicitWidth
                 + separator.implicitWidth
                 + titleLabel.implicitWidth
                 + content.spacing * 2
    implicitHeight: Math.max(
        appLabel.implicitHeight,
        separator.implicitHeight,
        titleLabel.implicitHeight
    )

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

    Row {
        id: content
        width: root.width
        height: root.implicitHeight
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spaceXs

        Text {
            id: appLabel
            width: Math.min(implicitWidth, Math.floor(root.width * 0.4))
            text: root._appName
            color: root._hasWindow ? Theme.fgSecondary : Theme.fgMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Theme.weightMedium
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }

        Text {
            id: separator
            text: "|"
            color: Theme.fgMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Theme.weightRegular
            verticalAlignment: Text.AlignVCenter
        }

        Text {
            id: titleLabel
            width: Math.max(
                0,
                root.width - appLabel.width - separator.implicitWidth - content.spacing * 2
            )
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
