// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Item {
    id: root

    property color color: "white"
    property string direction: "left"

    visible: width > 0 && height > 0

    function draw_cutout(ctx) {
        ctx.save();
        if (direction === "left")
            ctx.translate(0, root.height);
        else
            ctx.translate(root.width, root.height);
        ctx.scale(root.width, root.height);
        ctx.beginPath();
        ctx.arc(0, 0, 1, 0, Math.PI * 2);
        ctx.restore();
    }

    onColorChanged: shoulderCanvas.requestPaint()
    onDirectionChanged: shoulderCanvas.requestPaint()
    onWidthChanged: shoulderCanvas.requestPaint()
    onHeightChanged: shoulderCanvas.requestPaint()
    onVisibleChanged: shoulderCanvas.requestPaint()

    Canvas {
        id: shoulderCanvas
        anchors.fill: parent
        visible: root.visible
        antialiasing: true

        onPaint: {
            var ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);

            if (width <= 0 || height <= 0)
                return;

            ctx.fillStyle = root.color;
            ctx.fillRect(0, 0, width, height);
            ctx.globalCompositeOperation = "destination-out";
            root.draw_cutout(ctx);
            ctx.fill();
            ctx.globalCompositeOperation = "source-over";
        }
    }
}
