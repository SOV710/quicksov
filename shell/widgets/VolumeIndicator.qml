// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Item {
    id: root

    implicitWidth: Theme.statusCapsuleSlotWidth
    implicitHeight: Theme.statusCapsuleHeight

    signal clicked()

    readonly property string _iconPath: Theme.volumeIconFor(Audio.muted, Audio.volume)

    readonly property color _color: Audio.muted ? Theme.fgMuted : Theme.fgPrimary

    SvgIcon {
        anchors.centerIn: parent
        iconPath: root._iconPath
        size: Theme.statusIconSize
        color: root._color
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
        onWheel: function(wheel) {
            if (!Audio.ready || !Audio.defaultSink)
                return;
            var delta = wheel.angleDelta.y > 0 ? 0.05 : -0.05;
            var newVol = Math.max(0.0, Math.min(1.5, Audio.volume + delta));
            Audio.setVolume(Audio.defaultSink.id, newVol);
        }
    }
}
