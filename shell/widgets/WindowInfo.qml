// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    implicitWidth: Math.min(label.implicitWidth, 200)
    implicitHeight: label.implicitHeight

    property string _title: {
        if (!Niri.ready || !Niri.focusedWindow) return "";
        var w = Niri.focusedWindow;
        return w.title || w.app_id || "";
    }

    Text {
        id: label
        anchors.fill: parent
        text: root._title
        color: Theme.fgSecondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.weight: Theme.weightRegular
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }
}
