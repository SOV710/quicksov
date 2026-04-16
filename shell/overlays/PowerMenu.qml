// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../components"

Rectangle {
    id: root

    signal actionRequested(string actionId)
    signal closeRequested()

    implicitWidth: Theme.powerDockWidth
    implicitHeight: Theme.powerDockHeight
    radius: Theme.radiusLg
    color:  Theme.bgSurface
    border.color: Theme.borderDefault
    border.width: 1
    clip: true

    property string pendingActionId: ""

    readonly property var entries: [
        { id: "lock",     label: "Lock",     iconPath: "phosphor/lock.svg",              danger: false },
        { id: "suspend",  label: "Suspend",  iconPath: "phosphor/moon.svg",              danger: false },
        { id: "logout",   label: "Logout",   iconPath: "phosphor/sign-out.svg",          danger: false },
        { id: "reboot",   label: "Reboot",   iconPath: "phosphor/arrows-clockwise.svg",  danger: true  },
        { id: "shutdown", label: "Shutdown", iconPath: "phosphor/power.svg",             danger: true  }
    ]

    function _requiresConfirm(actionId) {
        return actionId === "reboot" || actionId === "shutdown";
    }

    function _triggerAction(actionId) {
        if (root._requiresConfirm(actionId) && root.pendingActionId !== actionId) {
            root.pendingActionId = actionId;
            confirmReset.restart();
            return;
        }

        confirmReset.stop();
        root.pendingActionId = "";
        root.actionRequested(actionId);
        root.closeRequested();
    }

    Timer {
        id: confirmReset
        interval: Theme.powerConfirmTimeoutMs
        onTriggered: root.pendingActionId = ""
    }

    Row {
        id: actionRow
        anchors.centerIn: parent
        spacing: Theme.spaceSm

        Repeater {
            model: root.entries

            delegate: PowerAction {
                required property var modelData
                entry: modelData
                width: Math.floor((root.width - actionRow.spacing * (root.entries.length - 1)) / root.entries.length)
                height: root.height - Theme.spaceSm * 2
                onTriggered: actionId => root._triggerAction(actionId)
            }
        }
    }

    component PowerAction: Item {
        id: actionItem

        property var entry: null
        readonly property bool pendingConfirm: root.pendingActionId === (entry ? entry.id : "")

        signal triggered(string actionId)

        Rectangle {
            id: iconPlate
            width: Theme.powerActionSize
            height: Theme.powerActionSize
            radius: Theme.radiusMd
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: parent.top
            color: actionMouse.containsMouse
                   ? (actionItem.pendingConfirm ? Theme.surfaceActive : Theme.surfaceHover)
                   : (actionItem.pendingConfirm ? Theme.surfaceActive : Qt.rgba(1, 1, 1, 0.04))
            border.width: 1
            border.color: actionItem.pendingConfirm ? Theme.colorError : Theme.borderDefault

            Behavior on color { ColorAnimation { duration: Theme.motionFast } }
            Behavior on border.color { ColorAnimation { duration: Theme.motionFast } }

            SvgIcon {
                anchors.centerIn: parent
                iconPath: actionItem.entry ? actionItem.entry.iconPath : "phosphor/power.svg"
                size: 28
                color: actionItem.pendingConfirm ? Theme.colorError : Theme.fgPrimary
            }
        }

        Text {
            width: parent.width
            anchors.top: iconPlate.bottom
            anchors.topMargin: Theme.spaceXs
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            text: actionItem.pendingConfirm ? "Click again" : (actionItem.entry ? actionItem.entry.label : "")
            color: actionItem.pendingConfirm ? Theme.colorError : Theme.fgSecondary
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            font.weight: actionItem.pendingConfirm ? Theme.weightMedium : Theme.weightRegular
        }

        MouseArea {
            id: actionMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: if (actionItem.entry) actionItem.triggered(actionItem.entry.id)
        }
    }
}
