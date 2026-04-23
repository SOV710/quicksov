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
    property real statusPreferredXOffset: -(Theme.statusDockLowerRadius * 1.5)
    property Component clockContentComponent: null
    property Component statusContentComponent: null
    readonly property var activePanelGeometry: {
        if (clockModel.open)
            return clockModel;
        if (statusModel.open)
            return statusModel;
        if (clockModel.active)
            return clockModel;
        if (statusModel.active)
            return statusModel;
        return null;
    }

    readonly property alias clockPanel: clockModel
    readonly property alias statusPanel: statusModel
    readonly property alias shellRegion: shellRegionItem

    PanelBackgroundField {
        id: backgroundField
        anchors.fill: parent
        shellModel: shellModel
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
        preferredXOffset: root.statusPreferredXOffset
        preferredWidth: root.statusPreferredWidth
        availableWidth: root.availableWidth
        maxBodyHeight: root.statusMaxBodyHeight
        contentImplicitHeight: statusSlot.contentImplicitHeight
        open: root.controller ? root.controller.statusPopup !== "" : false
    }

    PanelShellModel {
        id: shellModel
        coordinateItem: root
        barItem: root.barItem
        geometry: root.activePanelGeometry
    }

    PanelShellRegion {
        id: shellRegionItem
        shellModel: shellModel
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
}
