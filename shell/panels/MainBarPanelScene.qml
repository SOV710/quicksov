// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Item {
    id: root

    property Item barItem: null
    property Item clockTriggerItem: null
    property Item statusTriggerItem: null
    property var controller: null
    property real availableWidth: 0
    property real clockPreferredWidth: 0
    property real clockMaxBodyHeight: 0
    property real statusPreferredWidth: Theme.rightPopupWidth
    property real statusMaxBodyHeight: 0
    property Component clockContentComponent: null
    property Component statusContentComponent: null

    readonly property alias clockPanel: clockModel
    readonly property alias statusPanel: statusModel
    readonly property alias clockSurfaceItem: clockSurfaceItem
    readonly property alias statusSurfaceItem: statusSurfaceItem

    function repaintBackground() {
        backgroundField.requestPaint();
    }

    PanelBackgroundField {
        id: backgroundField
        anchors.fill: parent
        barItem: root.barItem
        panelModels: [clockModel, statusModel]
    }

    PanelGeometryModel {
        id: clockModel
        coordinateItem: root
        barItem: root.barItem
        triggerItem: root.clockTriggerItem
        alignmentMode: "center"
        preferredWidth: root.clockPreferredWidth
        availableWidth: root.availableWidth
        maxBodyHeight: root.clockMaxBodyHeight
        contentImplicitHeight: clockSlot.contentImplicitHeight
        open: root.controller ? root.controller.clockOpen : false
    }

    PanelGeometryModel {
        id: statusModel
        coordinateItem: root
        barItem: root.barItem
        triggerItem: root.statusTriggerItem
        alignmentMode: "right"
        preferredWidth: root.statusPreferredWidth
        availableWidth: root.availableWidth
        maxBodyHeight: root.statusMaxBodyHeight
        contentImplicitHeight: statusSlot.contentImplicitHeight
        open: root.controller ? root.controller.statusPopup !== "" : false
    }

    Item {
        id: clockSurfaceItem
        x: clockModel.x - clockModel.topLeftRadius
        y: clockModel.y
        width: clockModel.width + clockModel.topLeftRadius + clockModel.topRightRadius
        height: clockModel.height
        visible: false
    }

    Item {
        id: statusSurfaceItem
        x: statusModel.x - statusModel.topLeftRadius
        y: statusModel.y
        width: statusModel.width + statusModel.topLeftRadius + statusModel.topRightRadius
        height: statusModel.height
        visible: false
    }

    PanelContentSlot {
        id: clockSlot
        z: 2
        geometry: clockModel
        contentComponent: root.clockContentComponent
    }

    PanelContentSlot {
        id: statusSlot
        z: 2
        geometry: statusModel
        contentComponent: root.statusContentComponent
    }

    Connections {
        target: clockModel
        function onXChanged() { root.repaintBackground(); }
        function onYChanged() { root.repaintBackground(); }
        function onWidthChanged() { root.repaintBackground(); }
        function onHeightChanged() { root.repaintBackground(); }
        function onBodyHeightChanged() { root.repaintBackground(); }
        function onActiveChanged() { root.repaintBackground(); }
        function onTopLeftRadiusChanged() { root.repaintBackground(); }
        function onTopRightRadiusChanged() { root.repaintBackground(); }
    }

    Connections {
        target: statusModel
        function onXChanged() { root.repaintBackground(); }
        function onYChanged() { root.repaintBackground(); }
        function onWidthChanged() { root.repaintBackground(); }
        function onHeightChanged() { root.repaintBackground(); }
        function onBodyHeightChanged() { root.repaintBackground(); }
        function onActiveChanged() { root.repaintBackground(); }
        function onTopLeftRadiusChanged() { root.repaintBackground(); }
        function onTopRightRadiusChanged() { root.repaintBackground(); }
    }
}
