// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Canvas {
    id: root

    property Item barItem: null
    property var panelModels: []
    property color fillColor: Theme.barShellFill
    property color strokeColor: Theme.barShellBorder

    antialiasing: true

    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onBarItemChanged: requestPaint()
    onPanelModelsChanged: requestPaint()
    onFillColorChanged: requestPaint()
    onStrokeColorChanged: requestPaint()

    function _barRect() {
        if (!barItem)
            return { x: 0, y: 0, w: 0, h: 0 };
        var p = barItem.mapToItem(root, 0, 0);
        return { x: p.x, y: p.y, w: barItem.width, h: barItem.height };
    }

    function _roundedRectPath(ctx, x, y, w, h, r) {
        r = Math.max(0, Math.min(r, w / 2, h / 2));
        ctx.beginPath();
        ctx.moveTo(x + r, y);
        ctx.lineTo(x + w - r, y);
        ctx.quadraticCurveTo(x + w, y, x + w, y + r);
        ctx.lineTo(x + w, y + h - r);
        ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
        ctx.lineTo(x + r, y + h);
        ctx.quadraticCurveTo(x, y + h, x, y + h - r);
        ctx.lineTo(x, y + r);
        ctx.quadraticCurveTo(x, y, x + r, y);
        ctx.closePath();
    }

    function _activePanelModel() {
        for (var i = 0; i < root.panelModels.length; ++i) {
            var m = root.panelModels[i];
            if (m && m.active && m.width > 0 && m.height > 0)
                return m;
        }
        return null;
    }

    function _combinedShellPath(ctx, bar, m) {
        if (!m) {
            _roundedRectPath(ctx, bar.x, bar.y, bar.w, bar.h, Theme.barRadius);
            return;
        }

        var bx = bar.x;
        var by = bar.y;
        var bw = bar.w;
        var bh = bar.h;
        var br = Math.max(0, Math.min(Theme.barRadius, bw / 2, bh / 2));
        var attachY = by + bh;

        var x = m.x;
        var y = m.y;
        var w = m.width;
        var h = m.height;
        var sl = Math.min(m.topLeftRadius, Math.max(0, h));
        var sr = Math.min(m.topRightRadius, Math.max(0, h));
        var pr = Math.min(m.lowerRadius, Math.max(0, h / 2), Math.max(0, w / 2));
        var leftAttach = x - sl;
        var rightAttach = x + w + sr;

        ctx.beginPath();
        ctx.moveTo(bx + br, by);
        ctx.lineTo(bx + bw - br, by);
        ctx.quadraticCurveTo(bx + bw, by, bx + bw, by + br);
        ctx.lineTo(bx + bw, by + bh - br);
        ctx.quadraticCurveTo(bx + bw, by + bh, bx + bw - br, by + bh);

        ctx.lineTo(rightAttach, attachY);
        if (sr > 0)
            ctx.quadraticCurveTo(x + w, attachY, x + w, y + sr);
        else
            ctx.lineTo(x + w, y);

        ctx.lineTo(x + w, y + h - pr);
        if (pr > 0)
            ctx.quadraticCurveTo(x + w, y + h, x + w - pr, y + h);
        else
            ctx.lineTo(x + w, y + h);

        ctx.lineTo(x + pr, y + h);
        if (pr > 0)
            ctx.quadraticCurveTo(x, y + h, x, y + h - pr);
        else
            ctx.lineTo(x, y + h);

        ctx.lineTo(x, y + sl);
        if (sl > 0)
            ctx.quadraticCurveTo(x, attachY, leftAttach, attachY);
        else
            ctx.lineTo(x, y);

        ctx.lineTo(bx + br, attachY);
        ctx.quadraticCurveTo(bx, attachY, bx, by + bh - br);
        ctx.lineTo(bx, by + br);
        ctx.quadraticCurveTo(bx, by, bx + br, by);
        ctx.closePath();
    }

    onPaint: {
        var ctx = getContext("2d");
        ctx.reset();

        var bar = _barRect();
        if (bar.w <= 0 || bar.h <= 0)
            return;

        var activePanel = _activePanelModel();
        _combinedShellPath(ctx, bar, activePanel);
        ctx.fillStyle = root.fillColor;
        ctx.fill();
        ctx.lineWidth = 1;
        ctx.strokeStyle = root.strokeColor;
        ctx.stroke();
    }
}
