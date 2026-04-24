// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Layouts
import ".."
import "../components"

Item {
    id: root

    property var notif: null
    property bool expanded: false
    property bool interactive: true
    property bool showChevron: true
    property bool showDismissAction: true
    property string relativeTime: ""
    property int iconColumnWidth: 52

    property alias summaryArea: summaryArea

    signal actionRequested(string actionId)
    signal dismissRequested()

    readonly property var actions: _displayActions(notif ? notif.actions : [])
    readonly property color accentColor: _accentFor(notif ? notif.urgency : "normal")
    readonly property real contentColumnX: root.iconColumnWidth + Theme.spaceMd
    readonly property string detailsText: notif && typeof notif.body === "string" ? notif.body : ""
    readonly property string iconSource: notif && typeof notif.icon === "string" ? notif.icon : ""
    readonly property bool showActionsRow: root.expanded
                                           && (root.actions.length > 0 || root.showDismissAction)
    readonly property string titleText: _titleFor(notif)

    implicitHeight: contentCol.implicitHeight

    function _accentFor(urgency) {
        if (urgency === "critical") return Theme.colorError;
        if (urgency === "low") return Theme.fgMuted;
        return Theme.accentBlue;
    }

    function _displayActions(actions) {
        if (!actions || !actions.length) return [];

        return actions.filter(function(action) {
            return action
                && typeof action.id === "string"
                && typeof action.label === "string"
                && action.label.trim().length > 0;
        });
    }

    function _titleFor(notif) {
        if (!notif) return "Notification";
        if (notif.summary && notif.summary.length > 0) return notif.summary;
        if (notif.app_name && notif.app_name.length > 0) return notif.app_name;
        return "Notification";
    }

    Column {
        id: contentCol

        anchors.fill: parent
        spacing: Theme.spaceSm

        Item {
            id: summaryArea

            width: parent.width
            implicitHeight: summaryRow.implicitHeight
            property bool tapBlockedForCurrentGesture: false

            RowLayout {
                id: summaryRow

                width: parent.width
                spacing: Theme.spaceMd

                Rectangle {
                    Layout.alignment: Qt.AlignTop
                    Layout.preferredHeight: root.iconColumnWidth
                    Layout.preferredWidth: root.iconColumnWidth
                    radius: Theme.radiusMd
                    color: Theme.overlay(Theme.chromeSubtleFillMuted, root.accentColor, 0.22)
                    border.color: Theme.withAlpha(root.accentColor, 0.26)
                    border.width: 1

                    Image {
                        id: iconImage

                        anchors.centerIn: parent
                        width: 30
                        height: 30
                        asynchronous: true
                        fillMode: Image.PreserveAspectFit
                        mipmap: true
                        smooth: true
                        source: root.iconSource
                        visible: root.iconSource !== "" && status === Image.Ready
                    }

                    SvgIcon {
                        anchors.centerIn: parent
                        iconPath: Theme.iconNotificationStatus
                        size: 24
                        color: root.accentColor
                        visible: !iconImage.visible
                    }
                }

                ColumnLayout {
                    Layout.alignment: Qt.AlignTop
                    Layout.fillWidth: true
                    spacing: 6

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spaceSm

                        Text {
                            Layout.fillWidth: true
                            color: Theme.fgPrimary
                            elide: Text.ElideRight
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody
                            font.weight: Theme.weightSemibold
                            text: root.titleText
                        }

                        Text {
                            color: Theme.fgMuted
                            font.family: Theme.fontFamily
                            font.features: { "tnum": 1 }
                            font.pixelSize: Theme.fontSmall
                            text: root.relativeTime
                            visible: text !== ""
                        }

                        SvgIcon {
                            iconPath: "lucide/chevron-down.svg"
                            size: 16
                            color: Theme.fgMuted
                            rotation: root.expanded ? 180 : 0
                            visible: root.showChevron

                            Behavior on rotation {
                                NumberAnimation {
                                    duration: Theme.motionFast
                                    easing.type: Easing.OutCubic
                                }
                            }
                        }
                    }

                    Text {
                        Layout.fillWidth: true
                        visible: root.detailsText !== ""
                        color: Theme.fgSecondary
                        elide: Text.ElideRight
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        maximumLineCount: root.expanded ? 0 : 2
                        text: root.detailsText
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }

        Item {
            visible: root.showActionsRow
            width: parent.width
            implicitHeight: actionsContainer.implicitHeight

            Item {
                id: actionsContainer

                x: root.contentColumnX
                width: Math.max(0, parent.width - root.contentColumnX)
                implicitHeight: actionsRow.implicitHeight

                Row {
                    id: actionsRow

                    spacing: Theme.spaceXs

                    Repeater {
                        model: root.actions

                        delegate: NotificationActionChip {
                            required property var modelData

                            accentColor: root.accentColor
                            interactive: root.interactive
                            label: modelData.label || ""
                            onClicked: root.actionRequested(modelData.id)
                        }
                    }

                    NotificationActionChip {
                        accentColor: Theme.accentTeal
                        emphasized: true
                        interactive: root.interactive
                        label: "I got it"
                        visible: root.showDismissAction
                        onClicked: root.dismissRequested()
                    }
                }
            }
        }
    }
}
