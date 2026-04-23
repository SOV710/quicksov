// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell

Region {
    id: root

    property var shellModel: null
    readonly property var primitives: shellModel ? shellModel.outer : null

    function regionX(x) {
        return Math.floor(x);
    }

    function regionY(y) {
        return Math.floor(y);
    }

    function regionWidth(x, width) {
        return Math.max(0, Math.ceil((x + width) - regionX(x)));
    }

    function regionHeight(y, height) {
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
    }

}
