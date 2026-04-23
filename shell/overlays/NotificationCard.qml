// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Layouts
import ".."
import "../components"
import "../services"

Item {
    id: root

    property var notif: null
    property bool expanded: false
    property int cardIndex: -1
    property real neighborOffset: 0
    property string relativeTime: ""

    signal dismissRequested(int notificationId)
    signal dragStateChanged(int notificationId, int cardIndex, bool active, real progress)
    signal toggleExpandedRequested(int notificationId)

    readonly property var actions: _displayActions(notif ? notif.actions : [])
    readonly property color accentColor: _accentFor(notif ? notif.urgency : "normal")
    readonly property real dismissThreshold: Math.max(96, width * 0.35)
    readonly property string detailsText: notif && typeof notif.body === "string" ? notif.body : ""
    readonly property string iconSource: notif && typeof notif.icon === "string" ? notif.icon : ""
    readonly property string titleText: _titleFor(notif)

    property bool dismissing: false
    property real dragStartOffset: 0
    property real swipeOffset: 0

    Behavior on neighborOffset {
        SpringAnimation {
            spring: 4.2
            damping: 0.24
            mass: 0.8
            epsilon: 0.1
        }
    }

    Behavior on swipeOffset {
        enabled: !dragHandler.active && !root.dismissing

        SpringAnimation {
            spring: 3.4
            damping: 0.28
            mass: 0.9
            epsilon: 0.1
        }
    }

    implicitHeight: cardFrame.implicitHeight
    height: implicitHeight
    width: parent ? parent.width : 0

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

    function _dragProgressFor(offset) {
        if (root.dismissThreshold <= 0) return 0;
        return Math.max(0, Math.min(1, offset / root.dismissThreshold));
    }

    function _endDrag() {
        var id = root.notif ? root.notif.id : -1;
        var shouldDismiss = root.swipeOffset >= root.dismissThreshold;
        root.dragStateChanged(id, root.cardIndex, false, 0);

        if (shouldDismiss) {
            root.dismissing = true;
            dismissAnimation.from = root.swipeOffset;
            dismissAnimation.to = root.width + Theme.spaceXl;
            dismissAnimation.start();
            return;
        }

        root.swipeOffset = 0;
    }

    function _titleFor(notif) {
        if (!notif) return "Notification";
        if (notif.summary && notif.summary.length > 0) return notif.summary;
        if (notif.app_name && notif.app_name.length > 0) return notif.app_name;
        return "Notification";
    }

    Rectangle {
        id: cardFrame

        width: root.width
        x: root.swipeOffset + root.neighborOffset
        implicitHeight: contentCol.implicitHeight + Theme.spaceMd * 2
        radius: Theme.radiusMd
        color: cardHover.containsMouse ? Theme.surfaceHover : Theme.chromeSubtleFill
        border.color: root.notif && root.notif.urgency === "critical"
                      ? Theme.dangerBorderSoft
                      : (root.expanded ? Theme.borderDefault : Theme.borderSubtle)
        border.width: 1

        Behavior on color {
            ColorAnimation {
                duration: Theme.motionFast
            }
        }

        HoverHandler {
            id: cardHover
        }

        Column {
            id: contentCol

            anchors.fill: parent
            anchors.margins: Theme.spaceMd
            spacing: Theme.spaceSm

            Item {
                id: summaryArea

                width: parent.width
                implicitHeight: summaryRow.implicitHeight

                HoverHandler {
                    cursorShape: Qt.PointingHandCursor
                }

                TapHandler {
                    enabled: !root.dismissing
                    onTapped: {
                        if (root.notif)
                            root.toggleExpandedRequested(root.notif.id);
                    }
                }

                DragHandler {
                    id: dragHandler

                    target: null
                    xAxis.enabled: true
                    yAxis.enabled: false

                    onActiveChanged: {
                        if (active) {
                            root.dragStartOffset = root.swipeOffset;
                            if (root.notif)
                                root.dragStateChanged(
                                    root.notif.id,
                                    root.cardIndex,
                                    true,
                                    root._dragProgressFor(root.swipeOffset)
                                );
                            return;
                        }

                        if (!root.dismissing)
                            root._endDrag();
                    }

                    onTranslationChanged: {
                        if (!active) return;

                        var rawOffset = Math.max(0, root.dragStartOffset + translation.x);
                        if (rawOffset > root.dismissThreshold)
                            rawOffset = root.dismissThreshold + (rawOffset - root.dismissThreshold) * 0.32;
                        root.swipeOffset = rawOffset;

                        if (root.notif)
                            root.dragStateChanged(
                                root.notif.id,
                                root.cardIndex,
                                true,
                                root._dragProgressFor(root.swipeOffset)
                            );
                    }
                }

                RowLayout {
                    id: summaryRow

                    width: parent.width
                    spacing: Theme.spaceMd

                    Rectangle {
                        Layout.alignment: Qt.AlignTop
                        Layout.preferredHeight: 52
                        Layout.preferredWidth: 52
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

            Flow {
                visible: root.expanded
                width: parent.width
                spacing: Theme.spaceXs

                Repeater {
                    model: root.actions

                    delegate: ActionChip {
                        required property var modelData

                        accentColor: root.accentColor
                        label: modelData.label || ""
                        onClicked: {
                            if (root.notif)
                                Notification.invokeAction(root.notif.id, modelData.id);
                        }
                    }
                }

                ActionChip {
                    accentColor: Theme.accentTeal
                    emphasized: true
                    label: "I got it"
                    onClicked: {
                        if (root.notif)
                            root.dismissRequested(root.notif.id);
                    }
                }
            }
        }
    }

    NumberAnimation {
        id: dismissAnimation

        target: root
        property: "swipeOffset"
        duration: Theme.motionNormal
        easing.type: Easing.OutCubic
        onStopped: {
            if (root.dismissing && root.notif)
                root.dismissRequested(root.notif.id);
        }
    }

    component ActionChip: Rectangle {
        id: chip

        property color accentColor: Theme.accentBlue
        property bool emphasized: false
        property string label: ""

        signal clicked()

        implicitHeight: 28
        implicitWidth: chipLabel.implicitWidth + Theme.spaceMd * 2
        radius: Theme.radiusSm
        border.color: emphasized ? Theme.withAlpha(accentColor, 0.44) : Theme.borderDefault
        border.width: 1
        color: chipHover.containsMouse
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
            text: chip.label
        }

        HoverHandler {
            id: chipHover
            cursorShape: Qt.PointingHandCursor
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: chip.clicked()
        }
    }
}
