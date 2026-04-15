// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Item {
    id: root

    implicitWidth: row.implicitWidth
    implicitHeight: row.implicitHeight

    readonly property string _iconPath: {
        if (!Audio.ready || !Audio.defaultSink) return "lucide/volume-x.svg";
        if (Audio.muted) return "lucide/volume-x.svg";
        var v = Audio.volume;
        if (v <= 0) return "lucide/volume-off.svg";
        if (v > 0.66) return "lucide/volume-2.svg";
        return "lucide/volume-1.svg";
    }

    Row {
        id: row
        spacing: Theme.spaceXs

        SvgIcon {
            iconPath: root._iconPath
            size: Theme.iconSize
            color: Audio.muted ? Theme.fgMuted : Theme.fgPrimary
            anchors.verticalCenter: parent.verticalCenter
        }

        Text {
            text: Audio.ready && Audio.defaultSink
                  ? Math.round(Audio.volume * 100) + "%"
                  : "—"
            color: Audio.muted ? Theme.fgMuted : Theme.fgPrimary
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.features: { "tnum": 1 }
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
        onWheel: function(wheel) {
            if (!Audio.ready || !Audio.defaultSink) return;
            var delta = wheel.angleDelta.y > 0 ? 0.05 : -0.05;
            var newVol = Math.max(0.0, Math.min(1.5, Audio.volume + delta));
            Audio.setVolume(Audio.defaultSink.name, newVol);
        }
    }
}
