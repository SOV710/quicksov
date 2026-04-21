// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property string outputName: ""
    property real _gooeyFromCenterX: 0
    property real _gooeyToCenterX: 0
    property real _gooeyProgress: 1
    property bool _gooeyReady: false

    readonly property real _gooeyDisplayCenterX: _gooeyFromCenterX
        + (_gooeyToCenterX - _gooeyFromCenterX) * _gooeyProgress

    implicitWidth: container.implicitWidth
    implicitHeight: container.implicitHeight

    function _focusedDotCenterX() {
        for (var i = 0; i < workspaceRepeater.count; i++) {
            var item = workspaceRepeater.itemAt(i);
            if (!item || !item.wsData || !item.wsData.focused)
                continue;

            return item.mapToItem(container, item.width / 2, item.height / 2).x;
        }

        return -1;
    }

    function _syncFocusedBlob() {
        var target = _focusedDotCenterX();
        if (target < 0) {
            if (!Niri.ready || workspaceRepeater.count === 0) {
                _gooeyReady = false;
                gooeyAnimation.stop();
            }
            return;
        }

        if (!_gooeyReady) {
            _gooeyFromCenterX = target;
            _gooeyToCenterX = target;
            _gooeyProgress = 1;
            _gooeyReady = true;
            return;
        }

        if (Math.abs(target - _gooeyToCenterX) < 0.5)
            return;

        var current = _gooeyDisplayCenterX;
        gooeyAnimation.stop();
        _gooeyFromCenterX = current;
        _gooeyToCenterX = target;
        _gooeyProgress = 0;
        gooeyAnimation.restart();
    }

    function _queueSyncFocusedBlob() {
        syncTimer.restart();
    }

    onOutputNameChanged: root._queueSyncFocusedBlob()

    Component.onCompleted: root._queueSyncFocusedBlob()

    Connections {
        target: Niri

        function onReadyChanged() {
            root._queueSyncFocusedBlob();
        }

        function onWorkspacesChanged() {
            root._queueSyncFocusedBlob();
        }
    }

    Timer {
        id: syncTimer
        interval: 16
        repeat: false
        onTriggered: root._syncFocusedBlob()
    }

    NumberAnimation {
        id: gooeyAnimation
        target: root
        property: "_gooeyProgress"
        from: 0
        to: 1
        duration: Theme.workspaceGooeyDuration
        easing.type: Easing.OutCubic

        onFinished: {
            root._gooeyFromCenterX = root._gooeyToCenterX;
            root._gooeyProgress = 1;
        }
    }

    Rectangle {
        id: container
        implicitWidth: row.implicitWidth + Theme.groupContainerPadX * 2
        implicitHeight: Theme.groupContainerHeight
        width: implicitWidth
        height: implicitHeight
        radius: Theme.groupContainerRadius
        color: Theme.workspaceContainerFill
        border.color: Theme.workspaceContainerBorder
        border.width: 1

        Row {
            id: row
            anchors.centerIn: parent
            spacing: Theme.spaceSm

            Repeater {
                id: workspaceRepeater
                model: Niri.ready ? Niri.workspacesForOutput(root.outputName) : []

                delegate: WorkspaceDot {
                    required property var modelData
                    wsData: modelData
                }
            }
        }

        ShaderEffect {
            anchors.fill: parent
            visible: root._gooeyReady
            blending: true

            property real itemWidth: width
            property real itemHeight: height
            property real fromCenterX: root._gooeyFromCenterX
            property real toCenterX: root._gooeyToCenterX
            property real progress: root._gooeyProgress
            property real activeHalfWidth: Theme.workspaceSpotSize * 0.68
            property real activeHalfHeight: Theme.workspaceSpotSize * 0.68
            property real mergeStrength: Theme.workspaceGooeyMergeStrength
            property vector4d blobColor: Qt.vector4d(
                Theme.workspaceSpotActive.r,
                Theme.workspaceSpotActive.g,
                Theme.workspaceSpotActive.b,
                Theme.workspaceSpotActive.a
            )

            fragmentShader: Qt.resolvedUrl("../shaders/qsb/workspace_gooey.frag.qsb")
        }
    }

    component WorkspaceDot: Item {
        property var wsData: null
        width: Theme.workspaceSpotSize
        height: Theme.workspaceSpotSize

        Rectangle {
            id: dotRect
            anchors.centerIn: parent
            width: Theme.workspaceSpotSize
            height: Theme.workspaceSpotSize
            radius: Theme.workspaceSpotSize / 2
            color: wsData && wsData.focused ? Theme.workspaceSpotActive
                 : wsData && wsData.windows > 0 ? Theme.workspaceSpotFilled
                 : Theme.workspaceSpotEmpty
            opacity: wsData && wsData.focused ? 0.36 : 1.0

            Behavior on color { ColorAnimation { duration: Theme.motionFast } }
            Behavior on opacity { NumberAnimation { duration: Theme.motionFast; easing.type: Easing.OutCubic } }
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                if (wsData) Niri.focusWorkspace(wsData.idx);
            }
        }
    }
}
