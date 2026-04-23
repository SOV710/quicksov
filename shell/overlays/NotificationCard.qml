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
    property bool collapsedOut: false
    property bool directFollowActive: false
    property bool motionLocked: false
    property real neighborOffset: 0
    property string relativeTime: ""

    signal dismissRequested(int notificationId)
    signal dragStarted(int notificationId, int cardIndex)
    signal dragOffsetChanged(int notificationId, int cardIndex, real offset)
    signal cancelReleaseRequested(int notificationId, int cardIndex)
    signal dismissFlyoutStarted(int notificationId, int cardIndex)
    signal dismissFlyoutCompleted(int notificationId, int cardIndex)
    signal toggleExpandedRequested(int notificationId)

    readonly property var actions: _displayActions(notif ? notif.actions : [])
    readonly property color accentColor: _accentFor(notif ? notif.urgency : "normal")
    readonly property real dismissThreshold: Math.max(80, width * 0.28)
    readonly property string detailsText: notif && typeof notif.body === "string" ? notif.body : ""
    readonly property real contentColumnX: root.iconColumnWidth + Theme.spaceMd
    readonly property string iconSource: notif && typeof notif.icon === "string" ? notif.icon : ""
    readonly property int iconColumnWidth: 52
    readonly property string titleText: _titleFor(notif)

    property bool dismissing: false
    property real dragStartOffset: 0
    property real swipeOffset: 0

    Behavior on neighborOffset {
        enabled: !root.directFollowActive

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
    height: root.collapsedOut ? 0 : implicitHeight
    width: parent ? parent.width : 0

    Behavior on height {
        NumberAnimation {
            duration: Theme.statusDockRevealDuration
            easing.type: Easing.OutCubic
        }
    }

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

    function _endDrag() {
        var id = root.notif ? root.notif.id : -1;
        var shouldDismiss = root.swipeOffset >= root.dismissThreshold;

        if (shouldDismiss) {
            root.dismissing = true;
            if (root.notif)
                root.dismissFlyoutStarted(root.notif.id, root.cardIndex);
            dismissAnimation.from = root.swipeOffset;
            dismissAnimation.to = root.width + Theme.spaceXl;
            dismissAnimation.start();
            return;
        }

        if (root.notif)
            root.cancelReleaseRequested(root.notif.id, root.cardIndex);
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
        height: root.height
        x: root.swipeOffset + root.neighborOffset
        implicitHeight: contentCol.implicitHeight + Theme.spaceMd * 2
        radius: Theme.radiusMd
        color: cardHover.containsMouse ? Theme.surfaceHover : Theme.chromeSubtleFill
        border.color: root.notif && root.notif.urgency === "critical"
                      ? Theme.dangerBorderSoft
                      : (root.expanded ? Theme.borderDefault : Theme.borderSubtle)
        border.width: 1
        clip: true

        Behavior on color {
            ColorAnimation {
                duration: Theme.motionFast
            }
        }

        HoverHandler {
            id: cardHover
        }

        onXChanged: {
            if ((dragHandler.active || root.dismissing) && root.notif)
                root.dragOffsetChanged(root.notif.id, root.cardIndex, root.swipeOffset);
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
                    cursorShape: root.motionLocked ? Qt.ArrowCursor : Qt.PointingHandCursor
                }

                TapHandler {
                    enabled: !root.motionLocked && !root.dismissing
                    onTapped: {
                        if (root.notif)
                            root.toggleExpandedRequested(root.notif.id);
                    }
                }

                DragHandler {
                    id: dragHandler

                    enabled: !root.motionLocked || active
                    target: null
                    xAxis.enabled: true
                    yAxis.enabled: false

                    onActiveChanged: {
                        if (active) {
                            root.dragStartOffset = root.swipeOffset;
                            if (root.notif) {
                                root.dragStarted(root.notif.id, root.cardIndex);
                                root.dragOffsetChanged(root.notif.id, root.cardIndex, root.swipeOffset);
                            }
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
                    }
                }

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
                visible: root.expanded
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

                            delegate: ActionChip {
                                required property var modelData

                                accentColor: root.accentColor
                                interactive: !root.motionLocked && !root.dismissing
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
                            interactive: !root.motionLocked && !root.dismissing
                            label: "I got it"
                            onClicked: {
                                if (root.notif)
                                    root.dismissRequested(root.notif.id);
                            }
                        }
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
            if (root.dismissing && root.notif) {
                root.dismissing = false;
                root.dismissFlyoutCompleted(root.notif.id, root.cardIndex);
            }
        }
    }

    component ActionChip: Rectangle {
        id: chip

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
        color: chipHover.containsMouse && chip.interactive
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
            cursorShape: chip.interactive ? Qt.PointingHandCursor : Qt.ArrowCursor
        }

        MouseArea {
            anchors.fill: parent
            enabled: chip.interactive
            cursorShape: chip.interactive ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: chip.clicked()
        }
    }
}
