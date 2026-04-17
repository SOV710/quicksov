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
            for (var p = 0; p < root.pointsData.length; ++p) {
                var point = root.pointsData[p];
                var px = root._xForHour(point.hour, plotLeft, plotWidth);
                var py = root._yForTemp(point.temperature_c, plotTop, plotHeight);
                if (p === 0)
                    ctx.moveTo(px, py);
                else
                    ctx.lineTo(px, py);
            }
            ctx.lineTo(plotLeft + plotWidth, plotTop + plotHeight);
            ctx.lineTo(plotLeft, plotTop + plotHeight);
            ctx.closePath();
            ctx.fillStyle = root._colorWithAlpha(root.lineColor, 0.12);
            ctx.fill();

            ctx.beginPath();
            for (var lp = 0; lp < root.pointsData.length; ++lp) {
                var linePoint = root.pointsData[lp];
                var lx = root._xForHour(linePoint.hour, plotLeft, plotWidth);
                var ly = root._yForTemp(linePoint.temperature_c, plotTop, plotHeight);
                if (lp === 0)
                    ctx.moveTo(lx, ly);
                else
                    ctx.lineTo(lx, ly);
            }
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
