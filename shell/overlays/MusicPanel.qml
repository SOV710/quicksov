// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"
import "../services"

Rectangle {
    id: root

    signal closeRequested()

    radius: Theme.radiusMd
    color:  Theme.bgSurface
    border.color: Theme.borderDefault
    border.width: 1
    opacity: visible ? Theme.opacityPanel : 0
    clip: true

    Behavior on opacity { NumberAnimation { duration: Theme.motionNormal } }

    property var _player: Mpris.activePlayer

    Column {
        anchors {
            fill: parent
            margins: Theme.spaceLg
        }
        spacing: Theme.spaceMd

        // Album art
        Rectangle {
            width:  parent.width
            height: parent.width
            radius: Theme.radiusSm
            color:  Theme.bgSurfaceRaised
            clip: true

            Image {
                anchors.fill: parent
                source: root._player && root._player.metadata && root._player.metadata.art_url
                        ? root._player.metadata.art_url : ""
                fillMode: Image.PreserveAspectCrop
                visible: status !== Image.Error && source !== ""
            }

            SvgIcon {
                anchors.centerIn: parent
                iconPath: "phosphor/music-note.svg"
                size: 40
                color: Theme.fgMuted
                visible: root._player === null || !root._player.metadata || !root._player.metadata.art_url
            }
        }

        // Track info
        Column {
            width: parent.width
            spacing: Theme.spaceXs

            Text {
                text: root._player ? (root._player.metadata ? root._player.metadata.title || "Unknown" : "Unknown") : "No player"
                color: Theme.fgPrimary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Theme.weightSemibold
                elide: Text.ElideRight
                width: parent.width
            }

            Text {
                text: {
                    if (!root._player || !root._player.metadata) return "";
                    var artists = root._player.metadata.artist;
                    return Array.isArray(artists) ? artists.join(", ") : (artists || "");
                }
                color: Theme.fgSecondary
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                elide: Text.ElideRight
                width: parent.width
            }
        }

        // Controls
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.spaceLg

            SvgIcon {
                iconPath: "phosphor/skip-back.svg"
                size: 24
                color: root._player && root._player.can_go_previous ? Theme.fgPrimary : Theme.fgMuted
                anchors.verticalCenter: parent.verticalCenter

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: if (root._player) Mpris.previous(root._player.bus_name)
                }
            }

            SvgIcon {
                iconPath: root._player && root._player.playback_status === "Playing"
                          ? "phosphor/pause.svg" : "phosphor/play.svg"
                size: 28
                color: root._player ? Theme.fgPrimary : Theme.fgMuted
                anchors.verticalCenter: parent.verticalCenter

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: if (root._player) Mpris.playPause(root._player.bus_name)
                }
            }

            SvgIcon {
                iconPath: "phosphor/skip-forward.svg"
                size: 24
                color: root._player && root._player.can_go_next ? Theme.fgPrimary : Theme.fgMuted
                anchors.verticalCenter: parent.verticalCenter

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: if (root._player) Mpris.next(root._player.bus_name)
                }
            }
        }
    }

    // Close button
    Item {
        width: 20; height: 20
        anchors { top: parent.top; right: parent.right; margins: Theme.spaceSm }

        SvgIcon {
            anchors.centerIn: parent
            iconPath: "lucide/x.svg"
            size: 12
            color: Theme.fgMuted
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.closeRequested()
        }
    }
}
