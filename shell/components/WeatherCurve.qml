// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."
import "../services"

Item {
    id: root

    property var hourlyPoints: []
    property color accentColor: Theme.accentBlue
    property color gridColor: Theme.borderSubtle
    property color axisTextColor: Theme.fgMuted
    property color lineColor: Theme.accentBlue
    property real currentHourFloat: {
        var now = Time.now;
        return now.getHours() + now.getMinutes() / 60.0 + now.getSeconds() / 3600.0;
    }
    property string currentTimeLabel: Qt.formatTime(Time.now, "HH:mm")

    readonly property var pointsData: _normalizedPoints()
    readonly property bool hasEnoughData: pointsData.length > 1
    readonly property real _minTempRaw: _rawBound(true)
    readonly property real _maxTempRaw: _rawBound(false)
    readonly property real _displayMinTemp: Math.floor((_minTempRaw === _maxTempRaw ? _minTempRaw - 1 : _minTempRaw) - 1)
    readonly property real _displayMaxTemp: Math.ceil((_maxTempRaw === _minTempRaw ? _maxTempRaw + 1 : _maxTempRaw) + 1)

    function _colorWithAlpha(color, alpha) {
        return Qt.rgba(color.r, color.g, color.b, alpha);
    }

    function _normalizedPoints() {
        var points = [];
        var seen = {};
        for (var i = 0; i < hourlyPoints.length; ++i) {
            var entry = hourlyPoints[i];
            if (!entry || typeof entry.time !== "string")
                continue;

            var hour = parseInt(entry.time.slice(11, 13), 10);
            var temp = Number(entry.temperature_c);
            if (isNaN(hour) || isNaN(temp) || hour < 0 || hour > 23 || seen[hour])
                continue;

            seen[hour] = true;
            points.push({
                hour: hour,
                temperature_c: temp
            });
        }

        points.sort(function(a, b) { return a.hour - b.hour; });
        return points;
    }

    function _rawBound(isMin) {
        if (pointsData.length === 0)
            return isMin ? 0 : 1;

        var value = pointsData[0].temperature_c;
        for (var i = 1; i < pointsData.length; ++i) {
            value = isMin
                ? Math.min(value, pointsData[i].temperature_c)
                : Math.max(value, pointsData[i].temperature_c);
        }
        return value;
    }

    function _tempAt(hourFloat) {
        if (pointsData.length === 0)
            return 0;

        if (hourFloat <= pointsData[0].hour)
            return pointsData[0].temperature_c;
        if (hourFloat >= pointsData[pointsData.length - 1].hour)
            return pointsData[pointsData.length - 1].temperature_c;

        for (var i = 0; i < pointsData.length - 1; ++i) {
            var a = pointsData[i];
            var b = pointsData[i + 1];
            if (hourFloat < a.hour || hourFloat > b.hour)
                continue;

            var span = b.hour - a.hour;
            if (span <= 0)
                return a.temperature_c;

            var t = (hourFloat - a.hour) / span;
            return a.temperature_c + (b.temperature_c - a.temperature_c) * t;
        }

        return pointsData[pointsData.length - 1].temperature_c;
    }

    function _xForHour(hourValue, plotLeft, plotWidth) {
        return plotLeft + (Math.max(0, Math.min(23, hourValue)) / 23.0) * plotWidth;
    }

    function _yForTemp(temp, plotTop, plotHeight) {
        var minTemp = _displayMinTemp;
        var maxTemp = _displayMaxTemp;
        var range = Math.max(1, maxTemp - minTemp);
        return plotTop + (1.0 - ((temp - minTemp) / range)) * plotHeight;
    }

    function _formatHour(hourValue) {
        var hour = Math.max(0, Math.min(23, Math.round(hourValue)));
        return hour.toString().padStart(2, "0");
    }

    function _plotPoints(plotLeft, plotTop, plotWidth, plotHeight) {
        var points = [];
        for (var i = 0; i < root.pointsData.length; ++i) {
            var point = root.pointsData[i];
            points.push({
                x: root._xForHour(point.hour, plotLeft, plotWidth),
                y: root._yForTemp(point.temperature_c, plotTop, plotHeight)
            });
        }
        return points;
    }

    function _monotoneTangents(points) {
        var count = points.length;
        if (count < 2)
            return [];

        var dx = [];
        var slope = [];
        var tangent = [];
        var i;

        for (i = 0; i < count - 1; ++i) {
            dx[i] = points[i + 1].x - points[i].x;
            if (dx[i] <= 0)
                dx[i] = 1;
            slope[i] = (points[i + 1].y - points[i].y) / dx[i];
        }

        tangent[0] = slope[0];
        for (i = 1; i < count - 1; ++i) {
            if (slope[i - 1] === 0 || slope[i] === 0 || slope[i - 1] * slope[i] <= 0) {
                tangent[i] = 0;
            } else {
                var w1 = 2 * dx[i] + dx[i - 1];
                var w2 = dx[i] + 2 * dx[i - 1];
                tangent[i] = (w1 + w2) / ((w1 / slope[i - 1]) + (w2 / slope[i]));
            }
        }
        tangent[count - 1] = slope[count - 2];

        for (i = 0; i < count - 1; ++i) {
            if (slope[i] === 0) {
                tangent[i] = 0;
                tangent[i + 1] = 0;
                continue;
            }

            var a = tangent[i] / slope[i];
            var b = tangent[i + 1] / slope[i];
            var sum = a * a + b * b;
            if (sum > 9) {
                var scale = 3 / Math.sqrt(sum);
                tangent[i] = scale * a * slope[i];
                tangent[i + 1] = scale * b * slope[i];
            }
        }

        return tangent;
    }

    function _traceMonotonePath(ctx, points) {
        if (!points.length)
            return;

        ctx.moveTo(points[0].x, points[0].y);
        if (points.length === 1)
            return;

        var tangent = root._monotoneTangents(points);
        for (var i = 0; i < points.length - 1; ++i) {
            var p0 = points[i];
            var p1 = points[i + 1];
            var h = p1.x - p0.x;
            var cp1x = p0.x + h / 3;
            var cp1y = p0.y + tangent[i] * h / 3;
            var cp2x = p1.x - h / 3;
            var cp2y = p1.y - tangent[i + 1] * h / 3;
            ctx.bezierCurveTo(cp1x, cp1y, cp2x, cp2y, p1.x, p1.y);
        }
    }

    function _requestPaint() {
        curveCanvas.requestPaint();
    }

    onHourlyPointsChanged: _requestPaint()
    onWidthChanged: _requestPaint()
    onHeightChanged: _requestPaint()
    onAccentColorChanged: _requestPaint()
    onGridColorChanged: _requestPaint()
    onAxisTextColorChanged: _requestPaint()
    onLineColorChanged: _requestPaint()
    onCurrentHourFloatChanged: _requestPaint()
    onCurrentTimeLabelChanged: _requestPaint()

    Connections {
        target: Time
        function onNowChanged() { root._requestPaint(); }
    }

    Canvas {
        id: curveCanvas
        anchors.fill: parent

        onPaint: {
            var ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);

            if (!root.hasEnoughData || width <= 0 || height <= 0)
                return;

            var leftPad = Theme.spaceXl + Theme.spaceXs;
            var rightPad = Theme.spaceSm;
            var topPad = Theme.spaceSm;
            var bottomPad = Theme.spaceXl + Theme.spaceXs;
            var plotLeft = leftPad;
            var plotTop = topPad;
            var plotWidth = Math.max(1, width - leftPad - rightPad);
            var plotHeight = Math.max(1, height - topPad - bottomPad);
            var plotPoints = root._plotPoints(plotLeft, plotTop, plotWidth, plotHeight);
            var ticks = [
                root._displayMaxTemp,
                (root._displayMaxTemp + root._displayMinTemp) / 2.0,
                root._displayMinTemp
            ];
            var primaryFamily = Theme.fontFamily.split(",")[0].trim();

            ctx.lineWidth = 1;
            ctx.strokeStyle = root._colorWithAlpha(root.gridColor, 0.9);
            ctx.fillStyle = root.axisTextColor;
            ctx.font = Theme.fontMicro + "px \"" + primaryFamily + "\"";
            ctx.textAlign = "right";
            ctx.textBaseline = "middle";

            for (var i = 0; i < ticks.length; ++i) {
                var tick = ticks[i];
                var y = root._yForTemp(tick, plotTop, plotHeight);

                ctx.beginPath();
                ctx.moveTo(plotLeft, y);
                ctx.lineTo(plotLeft + plotWidth, y);
                ctx.stroke();

                ctx.fillText(Math.round(tick) + "°", plotLeft - Theme.spaceSm, y);
            }

            ctx.beginPath();
            root._traceMonotonePath(ctx, plotPoints);
            ctx.lineTo(plotLeft + plotWidth, plotTop + plotHeight);
            ctx.lineTo(plotLeft, plotTop + plotHeight);
            ctx.closePath();
            ctx.fillStyle = root._colorWithAlpha(root.lineColor, 0.12);
            ctx.fill();

            ctx.beginPath();
            root._traceMonotonePath(ctx, plotPoints);
            ctx.strokeStyle = root.lineColor;
            ctx.lineWidth = 2;
            ctx.lineJoin = "round";
            ctx.lineCap = "round";
            ctx.stroke();

            var currentHour = root.currentHourFloat;
            var markerX = root._xForHour(currentHour, plotLeft, plotWidth);
            var markerY = root._yForTemp(root._tempAt(currentHour), plotTop, plotHeight);

            ctx.beginPath();
            ctx.moveTo(markerX, plotTop);
            ctx.lineTo(markerX, plotTop + plotHeight);
            ctx.strokeStyle = root._colorWithAlpha(root.accentColor, 0.45);
            ctx.lineWidth = 1;
            ctx.stroke();

            ctx.beginPath();
            ctx.arc(markerX, markerY, Theme.spaceXs, 0, Math.PI * 2);
            ctx.fillStyle = root.accentColor;
            ctx.fill();

            ctx.beginPath();
            ctx.arc(markerX, markerY, Theme.spaceSm - 1, 0, Math.PI * 2);
            ctx.strokeStyle = root._colorWithAlpha(root.accentColor, 0.18);
            ctx.lineWidth = 4;
            ctx.stroke();

            var xTicks = [0, 6, 12, 18, 23];
            ctx.textBaseline = "top";
            for (var xt = 0; xt < xTicks.length; ++xt) {
                var hour = xTicks[xt];
                var labelX = root._xForHour(hour, plotLeft, plotWidth);
                var active = Math.abs(hour - currentHour) < 0.5;
                ctx.fillStyle = active ? root.accentColor : root.axisTextColor;
                ctx.textAlign = xt === 0 ? "left" : (xt === xTicks.length - 1 ? "right" : "center");
                ctx.fillText(root._formatHour(hour), labelX, plotTop + plotHeight + Theme.spaceSm);
            }

            ctx.textAlign = "center";
            ctx.textBaseline = "bottom";
            ctx.fillStyle = root.accentColor;
            ctx.fillText(
                root.currentTimeLabel,
                Math.max(plotLeft + Theme.spaceMd, Math.min(plotLeft + plotWidth - Theme.spaceMd, markerX)),
                Math.max(plotTop + Theme.spaceMd, markerY - Theme.spaceMd)
            );
        }
    }
}
