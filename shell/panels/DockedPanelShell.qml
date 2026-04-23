// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Item {
    id: root

    property Item barItem: null
    property Item triggerItem: null
    property string alignmentMode: "right"
    property real preferredWidth: 0
    property real availableWidth: 0
    property real maxBodyHeight: 0
    property bool open: false
    property color fillColor: Theme.barShellFill
    property color strokeColor: Theme.barShellBorder
    property int shoulderDepth: Theme.statusDockShoulderDepth
    property int lowerRadius: Theme.statusDockLowerRadius
    property int seamOverlap: Theme.statusDockSeamOverlap
    property Component contentComponent: null
    property Component _retainedContentComponent: null

    readonly property real panelWidth: Math.max(0, Math.min(preferredWidth, availableWidth))
    readonly property real contentImplicitHeight: contentLoader.item ? contentLoader.item.implicitHeight : 0
    property real panelBodyHeight: open ? Math.min(contentImplicitHeight, maxBodyHeight) : 0
    readonly property bool shellVisible: open || panelBodyHeight > 0.5 || bodyHeightAnimation.running
    readonly property real barLeft: {
        if (barItem && parent) {
            var p = barItem.mapToItem(parent, 0, 0);
            return p.x;
        }
        return 0;
    }
    readonly property real barRight: {
        if (barItem && parent) {
            var p = barItem.mapToItem(parent, 0, 0);
            return p.x + barItem.width;
        }
        return panelX + panelWidth;
    }
    readonly property real preferredX: {
        if (triggerItem && parent) {
            var p = triggerItem.mapToItem(parent, 0, 0);
            if (alignmentMode === "center")
                return p.x + (triggerItem.width - panelWidth) / 2;
            return p.x + triggerItem.width - panelWidth;
        }
        return barRight - panelWidth;
    }
    readonly property real panelX: Math.max(barLeft, Math.min(barRight - panelWidth, preferredX))
    readonly property real topLeftRadius: Math.max(
        0,
        Math.min(shoulderDepth, panelX - barLeft)
    )
    readonly property real topRightRadius: Math.max(
        0,
        Math.min(shoulderDepth, barRight - (panelX + panelWidth))
    )

    x: panelX
    y: barItem ? barItem.y + barItem.height - seamOverlap : 0
    width: panelWidth
    height: shoulderDepth + panelBodyHeight
    visible: shellVisible

    Behavior on panelBodyHeight {
        NumberAnimation {
            id: bodyHeightAnimation
            duration: Theme.statusDockRevealDuration
            easing.type: Easing.OutCubic
        }
    }

    onWidthChanged: shellCanvas.requestPaint()
    onHeightChanged: shellCanvas.requestPaint()
    onFillColorChanged: shellCanvas.requestPaint()
    onStrokeColorChanged: shellCanvas.requestPaint()
    onTopLeftRadiusChanged: shellCanvas.requestPaint()
    onTopRightRadiusChanged: shellCanvas.requestPaint()
    onLowerRadiusChanged: shellCanvas.requestPaint()
    onShellVisibleChanged: shellCanvas.requestPaint()
    onContentComponentChanged: {
        if (contentComponent)
            _retainedContentComponent = contentComponent;
    }

    Canvas {
        id: shellCanvas
        anchors.fill: parent
        visible: root.shellVisible
        antialiasing: true

        function buildPath(ctx) {
            var w = width;
            var h = height;
            var tl = Math.min(root.topLeftRadius, Math.max(0, h));
            var tr = Math.min(root.topRightRadius, Math.max(0, h));
            var br = Math.min(root.lowerRadius, Math.max(0, h / 2), Math.max(0, w / 2));
            var bl = br;

            ctx.beginPath();
            ctx.moveTo(0, tl);
            if (tl > 0)
                ctx.quadraticCurveTo(0, 0, tl, 0);
            else
                ctx.lineTo(0, 0);

            ctx.lineTo(w - tr, 0);
            if (tr > 0)
                ctx.quadraticCurveTo(w, 0, w, tr);
            else
                ctx.lineTo(w, 0);

            ctx.lineTo(w, h - br);
            if (br > 0)
                ctx.quadraticCurveTo(w, h, w - br, h);
            else
                ctx.lineTo(w, h);

            ctx.lineTo(bl, h);
            if (bl > 0)
                ctx.quadraticCurveTo(0, h, 0, h - bl);
            else
                ctx.lineTo(0, h);

            ctx.closePath();
        }

        onPaint: {
            var ctx = getContext("2d");
            ctx.reset();
            if (!root.shellVisible || width <= 0 || height <= 0)
                return;

            buildPath(ctx);
            ctx.fillStyle = root.fillColor;
            ctx.fill();

            buildPath(ctx);
            ctx.lineWidth = 1;
            ctx.strokeStyle = root.strokeColor;
            ctx.stroke();
        }
    }

    Item {
        id: contentViewport
        x: 0
        y: root.shoulderDepth
        width: root.width
        height: root.panelBodyHeight
        clip: true
        visible: root.shellVisible

        Loader {
            id: contentLoader
            anchors.fill: parent
            active: root.open || root.shellVisible
            sourceComponent: root.contentComponent ? root.contentComponent : root._retainedContentComponent
        }
    }
}
