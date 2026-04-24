// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import ".."

Region {
    id: root

    property var shellModel: null
    readonly property var primitives: shellModel ? shellModel.outer : null
    readonly property string surfaceName: shellModel ? shellModel.surfaceName : "panel"

    function regionX(x) {
        if (DebugVisuals.disableRegionRounding)
            return x;
        return Math.floor(x);
    }

    function regionY(y) {
        if (DebugVisuals.disableRegionRounding)
            return y;
        return Math.floor(y);
    }

    function regionWidth(x, width) {
        if (DebugVisuals.disableRegionRounding)
            return Math.max(0, width);
        return Math.max(0, Math.ceil((x + width) - regionX(x)));
    }

    function regionHeight(y, height) {
        if (DebugVisuals.disableRegionRounding)
            return Math.max(0, height);
        return Math.max(0, Math.ceil((y + height) - regionY(y)));
    }

    x: primitives ? regionX(primitives.barX) : 0
    y: primitives ? regionY(primitives.barY) : 0
    width: primitives ? regionWidth(primitives.barX, primitives.barWidth) : 0
    height: primitives ? regionHeight(primitives.barY, primitives.barHeight) : 0
    radius: primitives ? primitives.barRadius : 0

    Region {
        x: root.primitives && root.primitives.panelActive ? root.regionX(root.primitives.neckX) : 0
        y: root.primitives && root.primitives.panelActive ? root.regionY(root.primitives.neckY) : 0
        width: root.primitives && root.primitives.panelActive
               ? root.regionWidth(root.primitives.neckX, root.primitives.neckWidth)
               : 0
        height: root.primitives && root.primitives.panelActive
                ? root.regionHeight(root.primitives.neckY, root.primitives.neckHeight)
                : 0
    }

    Region {
        id: bodyRegion

        x: root.primitives && root.primitives.panelActive ? root.regionX(root.primitives.bodyX) : 0
        y: root.primitives && root.primitives.panelActive ? root.regionY(root.primitives.bodyY) : 0
        width: root.primitives && root.primitives.panelActive
               ? root.regionWidth(root.primitives.bodyX, root.primitives.bodyWidth)
               : 0
        height: root.primitives && root.primitives.panelActive
                ? root.regionHeight(root.primitives.bodyY, root.primitives.bodyHeight)
                : 0
        topLeftRadius: 0
        topRightRadius: 0
        bottomLeftRadius: root.primitives ? root.primitives.bodyRadius : 0
        bottomRightRadius: root.primitives ? root.primitives.bodyRadius : 0

        onYChanged: root._logBodyRegion("body-region-y-changed")
        onHeightChanged: root._logBodyRegion("body-region-height-changed")
    }

    function _logBodyRegion(eventName) {
        DebugVisuals.logTransition(
            root.surfaceName,
            root.shellModel && root.shellModel.geometry && root.shellModel.geometry.open ? "popup-open" : "popup-close",
            {
            bodyRadius: root.primitives ? root.primitives.bodyRadius : 0,
            disableRegionRounding: DebugVisuals.disableRegionRounding,
            event: eventName,
            rawBodyHeight: root.primitives ? root.primitives.bodyHeight : 0,
            rawBodyY: root.primitives ? root.primitives.bodyY : 0,
            regionBodyHeight: bodyRegion.height,
            regionBodyY: bodyRegion.y
            }
        );
    }
}
