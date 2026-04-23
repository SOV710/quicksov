// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Layouts
import ".."
import "../components"
import "../services"

Item {
    id: root

    property int viewMonth: Time.now.getMonth()
    property int viewYear: Time.now.getFullYear()

    readonly property int _firstDayOfWeekJs: {
        var day = Qt.locale().firstDayOfWeek;
        return day === undefined ? 1 : (day % 7);
    }
    readonly property string _todayKey: Qt.formatDate(Time.now, "yyyy-MM-dd")
    readonly property string _weatherDayKey: _resolveWeatherDayKey()
    readonly property real _weatherCurrentHourFloat: _resolveWeatherCurrentHourFloat()
    readonly property string _weatherCurrentTimeLabel: _resolveWeatherCurrentTimeLabel()
    readonly property var _calendarCells: _buildCalendarCells()
    readonly property var _todayWeatherSeries: _buildTodayWeatherSeries()
    readonly property bool _weatherExpired: Weather.isExpired(Time.now.getTime())
    readonly property bool _hasUsableWeather: Weather.current !== null
                                             && _todayWeatherSeries.length > 1
                                             && !_weatherExpired
    readonly property string _weatherStatusLabel: _resolveWeatherStatusLabel()
    readonly property color _weatherStatusColor: _resolveWeatherStatusColor()
    readonly property string _locationLabel: {
        if (Weather.location && Weather.location.name)
            return Weather.location.name;
        return "Weather";
    }
    readonly property string _longDateLabel: Qt.formatDate(Time.now, "dddd · d MMMM yyyy")
    readonly property var _weatherMetrics: _buildWeatherMetrics()

    width: parent ? parent.width : Theme.clockPanelMaxWidth
    implicitWidth: width
    implicitHeight: Theme.clockPanelMaxHeight

    function _beginningOfCell(index) {
        var start = new Date(root.viewYear, root.viewMonth, 1);
        var offset = (start.getDay() - root._firstDayOfWeekJs + 7) % 7;
        start.setDate(start.getDate() - offset + index);
        return start;
    }

    function _buildCalendarCells() {
        var cells = [];
        for (var i = 0; i < 42; ++i) {
            var day = root._beginningOfCell(i);
            var key = Qt.formatDate(day, "yyyy-MM-dd");
            cells.push({
                year: day.getFullYear(),
                month: day.getMonth(),
                day: day.getDate(),
                key: key,
                inMonth: day.getMonth() === root.viewMonth,
                isToday: key === root._todayKey
            });
        }
        return cells;
    }

    function _buildTodayWeatherSeries() {
        var points = [];
        for (var i = 0; i < Weather.hourlyForecast.length; ++i) {
            var entry = Weather.hourlyForecast[i];
            if (!entry || typeof entry.time !== "string" || entry.time.slice(0, 10) !== root._weatherDayKey)
                continue;

            var hour = parseInt(entry.time.slice(11, 13), 10);
            var temp = Number(entry.temperature_c);
            if (isNaN(hour) || isNaN(temp))
                continue;

            points.push({
                time: entry.time,
                hour: hour,
                temperature_c: temp,
                wmo_code: entry.wmo_code
            });
        }
        points.sort(function(a, b) { return a.hour - b.hour; });
        return points;
    }

    function _resolveWeatherDayKey() {
        if (Weather.currentTimeIso && Weather.currentTimeIso.length >= 10)
            return Weather.currentTimeIso.slice(0, 10);
        if (Weather.hourlyForecast.length) {
            var first = Weather.hourlyForecast[0];
            if (first && typeof first.time === "string")
                return first.time.slice(0, 10);
        }
        return root._todayKey;
    }

    function _resolveWeatherCurrentHourFloat() {
        var iso = Weather.currentTimeIso;
        if (iso && iso.length >= 16) {
            var hour = parseInt(iso.slice(11, 13), 10);
            var minute = parseInt(iso.slice(14, 16), 10);
            if (!isNaN(hour) && !isNaN(minute))
                return hour + minute / 60.0;
        }

        var now = Time.now;
        return now.getHours() + now.getMinutes() / 60.0 + now.getSeconds() / 3600.0;
    }

    function _resolveWeatherCurrentTimeLabel() {
        var iso = Weather.currentTimeIso;
        if (iso && iso.length >= 16)
            return iso.slice(11, 16);
        return Qt.formatTime(Time.now, "HH:mm");
    }

    function _weekdayLabel(index) {
        var jsDay = (root._firstDayOfWeekJs + index) % 7;
        var dayDate = new Date(2024, 0, 7 + jsDay);
        var text = dayDate.toLocaleDateString(Qt.locale(), "ddd");
        text = text.replace(/\.$/, "");
        return text.slice(0, 2).toUpperCase();
    }

    function _shiftMonth(delta) {
        var target = new Date(root.viewYear, root.viewMonth + delta, 1);
        root.viewYear = target.getFullYear();
        root.viewMonth = target.getMonth();
    }

    function _resetToToday() {
        root.viewYear = Time.now.getFullYear();
        root.viewMonth = Time.now.getMonth();
    }

    function _formatTemp(value) {
        if (typeof value !== "number" || isNaN(value))
            return "--";
        return Math.round(value) + "°";
    }

    function _formatUpdateTime() {
        if (Weather.lastSuccessMs === null)
            return "--";
        return Qt.formatTime(new Date(Weather.lastSuccessMs), "HH:mm");
    }

    function _maybeRefreshWeather() {
        if (Weather.fetchStatus === "loading" || Weather.fetchStatus === "refreshing")
            return;

        if (Weather.current === null
                || root._todayWeatherSeries.length < 2
                || root._weatherExpired) {
            Weather.refresh();
        }
    }

    function _resolveWeatherStatusLabel() {
        if (Weather.fetchStatus === "refreshing")
            return "Refreshing";
        if (Weather.fetchStatus === "loading")
            return "Loading";
        if (root._weatherExpired)
            return "Expired";
        if (Weather.fetchStatus === "refresh_failed" && root._hasUsableWeather)
            return "Stale";
        if (Weather.fetchStatus === "init_failed" || Weather.fetchStatus === "refresh_failed")
            return "Unavailable";
        return "Ready";
    }

    function _resolveWeatherStatusColor() {
        if (Weather.fetchStatus === "refreshing" || Weather.fetchStatus === "loading")
            return Theme.accentBlue;
        if (Weather.fetchStatus === "init_failed" || root._weatherExpired)
            return Theme.colorError;
        if (Weather.fetchStatus === "refresh_failed")
            return Theme.colorWarning;
        return Theme.colorSuccess;
    }

    function _buildWeatherMetrics() {
        if (!root._hasUsableWeather || !Weather.current)
            return [];

        return [
            {
                iconPath: "lucide/droplets.svg",
                label: "Humidity",
                value: Math.round(Weather.current.humidity_pct) + "%"
            },
            {
                iconPath: "lucide/wind.svg",
                label: "Wind",
                value: Math.round(Weather.current.wind_kmh) + " km/h"
            },
            {
                iconPath: Weather.fetchStatus === "refresh_failed"
                    ? "lucide/triangle-alert.svg"
                    : "lucide/rotate-cw.svg",
                label: Weather.fetchStatus === "refresh_failed" ? "Status" : "Updated",
                value: Weather.fetchStatus === "refresh_failed" ? "Retry failed" : root._formatUpdateTime()
            }
        ];
    }

    Component.onCompleted: {
        root._resetToToday();
        root._maybeRefreshWeather();
    }

    Rectangle {
        id: panel
        anchors.fill: parent
        radius: 0
        color: "transparent"
        border.width: 0

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onClicked: function(mouse) { mouse.accepted = true; }
            onPressed: function(mouse) { mouse.accepted = true; }
        }

        RowLayout {
            anchors.fill: parent
            anchors.margins: Theme.spaceXl
            spacing: Theme.spaceXl

            Rectangle {
                id: calendarCard
                Layout.fillHeight: true
                Layout.preferredWidth: Math.floor((panel.width - Theme.spaceXl * 3) * 0.56)
                radius: Theme.radiusMd
                color: Theme.bgSurfaceRaised
                border.color: Theme.borderSubtle
                border.width: 1

                WheelHandler {
                    target: calendarCard
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                    onWheel: function(event) {
                        if (event.angleDelta.y > 0)
                            root._shiftMonth(-1);
                        else if (event.angleDelta.y < 0)
                            root._shiftMonth(1);
                        event.accepted = true;
                    }
                }

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spaceLg
                    spacing: Theme.spaceLg

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spaceSm

                        ColumnLayout {
                            spacing: 0

                            Text {
                                text: Qt.locale().monthName(root.viewMonth, Locale.LongFormat)
                                color: Theme.fgPrimary
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontDisplay
                                font.weight: Theme.weightSemibold
                            }

                            Text {
                                text: root.viewYear
                                color: Theme.fgSecondary
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.weight: Theme.weightMedium
                                font.features: { "tnum": 1 }
                            }
                        }

                        Item { Layout.fillWidth: true }

                        Rectangle {
                            id: todayButton
                            radius: Theme.radiusSm
                            color: todayHover.hovered ? Theme.surfaceHover : "transparent"
                            border.color: Theme.borderDefault
                            border.width: 1
                            implicitWidth: todayLabel.implicitWidth + Theme.spaceLg
                            implicitHeight: todayLabel.implicitHeight + Theme.spaceSm

                            Text {
                                id: todayLabel
                                anchors.centerIn: parent
                                text: "Today"
                                color: Theme.fgSecondary
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.weight: Theme.weightMedium
                            }

                            HoverHandler { id: todayHover }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root._resetToToday()
                            }
                        }

                        IconButton {
                            iconPath: "lucide/chevron-left.svg"
                            iconSize: Theme.iconSize
                            onClicked: root._shiftMonth(-1)
                        }

                        IconButton {
                            iconPath: "lucide/chevron-right.svg"
                            iconSize: Theme.iconSize
                            onClicked: root._shiftMonth(1)
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spaceXs

                        Repeater {
                            model: 7

                            delegate: Item {
                                required property int index
                                Layout.fillWidth: true
                                implicitHeight: weekdayLabel.implicitHeight

                                Text {
                                    id: weekdayLabel
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    text: root._weekdayLabel(index)
                                    color: Theme.fgMuted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontMicro
                                    font.weight: Theme.weightMedium
                                }
                            }
                        }
                    }

                    Item {
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        Grid {
                            id: calendarGrid
                            anchors.fill: parent
                            columns: 7
                            rowSpacing: Theme.spaceXs
                            columnSpacing: Theme.spaceXs

                            readonly property real cellWidth: (width - (columns - 1) * columnSpacing) / columns
                            readonly property real cellHeight: (height - 5 * rowSpacing) / 6

                            Repeater {
                                model: root._calendarCells

                                delegate: Rectangle {
                                    required property var modelData

                                    width: calendarGrid.cellWidth
                                    height: calendarGrid.cellHeight
                                    radius: Theme.radiusSm
                                    color: cellHover.hovered
                                           ? Theme.surfaceHover
                                           : modelData.isToday
                                             ? Theme.surfaceActive
                                             : "transparent"
                                    border.color: modelData.isToday ? Theme.borderAccent : Theme.borderSubtle
                                    border.width: modelData.isToday ? 1 : 0

                                    HoverHandler { id: cellHover }

                                    Text {
                                        anchors.centerIn: parent
                                        text: modelData.day
                                        color: modelData.inMonth
                                               ? Theme.fgPrimary
                                               : Theme.fgMuted
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontBody
                                        font.weight: modelData.isToday
                                                     ? Theme.weightSemibold
                                                     : modelData.inMonth ? Theme.weightMedium : Theme.weightRegular
                                        font.features: { "tnum": 1 }
                                    }
                                }
                            }
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spaceSm

                        SvgIcon {
                            iconPath: "lucide/calendar-days.svg"
                            size: Theme.iconSize
                            color: Theme.fgMuted
                        }

                        Text {
                            Layout.fillWidth: true
                            text: root._longDateLabel
                            color: Theme.fgSecondary
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody
                            font.weight: Theme.weightRegular
                            elide: Text.ElideRight
                        }
                    }
                }
            }

            Rectangle {
                id: weatherCard
                Layout.fillHeight: true
                Layout.fillWidth: true
                radius: Theme.radiusMd
                color: Theme.bgSurfaceRaised
                border.color: Theme.borderSubtle
                border.width: 1

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spaceLg
                    spacing: Theme.spaceLg

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spaceSm

                        SvgIcon {
                            iconPath: "lucide/map-pin.svg"
                            size: Theme.iconSize
                            color: Theme.fgMuted
                        }

                        Text {
                            Layout.fillWidth: true
                            text: root._locationLabel
                            color: Theme.fgPrimary
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontLabel
                            font.weight: Theme.weightMedium
                            elide: Text.ElideRight
                        }

                        Rectangle {
                            radius: Theme.radiusSm
                            color: Qt.rgba(root._weatherStatusColor.r, root._weatherStatusColor.g, root._weatherStatusColor.b, 0.12)
                            border.color: Qt.rgba(root._weatherStatusColor.r, root._weatherStatusColor.g, root._weatherStatusColor.b, 0.35)
                            border.width: 1
                            implicitWidth: statusLabel.implicitWidth + Theme.spaceMd
                            implicitHeight: statusLabel.implicitHeight + Theme.spaceXs

                            Text {
                                id: statusLabel
                                anchors.centerIn: parent
                                text: root._weatherStatusLabel
                                color: root._weatherStatusColor
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontMicro
                                font.weight: Theme.weightMedium
                            }
                        }

                        Rectangle {
                            id: refreshButton
                            width: Theme.barHeight
                            height: Theme.barHeight
                            radius: Theme.radiusSm
                            color: refreshHover.hovered ? Theme.surfaceHover : "transparent"
                            border.color: Theme.borderDefault
                            border.width: 1

                            HoverHandler { id: refreshHover }

                            SvgIcon {
                                id: refreshIcon
                                anchors.centerIn: parent
                                iconPath: Weather.fetchStatus === "loading" || Weather.fetchStatus === "refreshing"
                                          ? "lucide/loader-circle.svg"
                                          : "lucide/rotate-cw.svg"
                                size: Theme.iconSize
                                color: Theme.fgSecondary

                                RotationAnimator on rotation {
                                    loops: Animation.Infinite
                                    from: 0
                                    to: 360
                                    duration: Theme.motionDeliberate * 4
                                    running: Weather.fetchStatus === "loading" || Weather.fetchStatus === "refreshing"
                                }
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Weather.refresh()
                            }
                        }
                    }

                    Item {
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        ColumnLayout {
                            anchors.fill: parent
                            spacing: Theme.spaceLg
                            visible: root._hasUsableWeather

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceLg

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0

                                    Text {
                                        text: Weather.current ? root._formatTemp(Weather.current.temperature_c) : "--"
                                        color: Theme.fgPrimary
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontHero
                                        font.weight: Theme.weightSemibold
                                        font.features: { "tnum": 1 }
                                    }

                                    Text {
                                        text: Weather.current ? Weather.current.description : ""
                                        color: Theme.fgSecondary
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontBody
                                        font.weight: Theme.weightMedium
                                        elide: Text.ElideRight
                                    }

                                    Text {
                                        text: Weather.current
                                              ? ("Feels like " + root._formatTemp(Weather.current.apparent_c)
                                                 + (Weather.timezoneAbbreviation ? (" · " + Weather.timezoneAbbreviation) : ""))
                                              : ""
                                        color: Theme.fgMuted
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontMicro
                                        font.weight: Theme.weightRegular
                                        font.features: { "tnum": 1 }
                                    }
                                }

                                Rectangle {
                                    width: Theme.clockWeatherIconSize + Theme.spaceLg
                                    height: width
                                    radius: Theme.radiusMd
                                    color: Theme.surfaceHover
                                    border.color: Theme.borderSubtle
                                    border.width: 1

                                    SvgIcon {
                                        anchors.centerIn: parent
                                        iconPath: Weather.current
                                                  ? ("lucide/" + Weather.current.icon + ".svg")
                                                  : "lucide/cloud.svg"
                                        size: Theme.clockWeatherIconSize
                                        color: Theme.accentBlue
                                    }
                                }
                            }

                            WeatherCurve {
                                Layout.fillWidth: true
                                Layout.preferredHeight: Theme.clockWeatherChartHeight
                                hourlyPoints: root._todayWeatherSeries
                                accentColor: Theme.accentBlue
                                gridColor: Theme.borderDefault
                                axisTextColor: Theme.fgMuted
                                lineColor: Theme.accentBlue
                                currentHourFloat: root._weatherCurrentHourFloat
                                currentTimeLabel: root._weatherCurrentTimeLabel
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceSm

                                Repeater {
                                    model: root._weatherMetrics

                                    delegate: Rectangle {
                                        required property var modelData

                                        Layout.fillWidth: true
                                        implicitHeight: metricText.implicitHeight + metricLabel.implicitHeight + Theme.spaceMd
                                        radius: Theme.radiusSm
                                        color: Theme.bgSurface
                                        border.color: Theme.borderSubtle
                                        border.width: 1

                                        RowLayout {
                                            anchors.fill: parent
                                            anchors.margins: Theme.spaceSm
                                            spacing: Theme.spaceSm

                                            SvgIcon {
                                                iconPath: modelData.iconPath
                                                size: Theme.iconSize
                                                color: Theme.fgSecondary
                                            }

                                            ColumnLayout {
                                                Layout.fillWidth: true
                                                spacing: 0

                                                Text {
                                                    id: metricLabel
                                                    text: modelData.label
                                                    color: Theme.fgMuted
                                                    font.family: Theme.fontFamily
                                                    font.pixelSize: Theme.fontMicro
                                                    font.weight: Theme.weightRegular
                                                }

                                                Text {
                                                    id: metricText
                                                    text: modelData.value
                                                    color: Theme.fgPrimary
                                                    font.family: Theme.fontFamily
                                                    font.pixelSize: Theme.fontBody
                                                    font.weight: Theme.weightMedium
                                                    font.features: { "tnum": 1 }
                                                    elide: Text.ElideRight
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        ColumnLayout {
                            anchors.centerIn: parent
                            spacing: Theme.spaceSm
                            visible: !root._hasUsableWeather

                            SvgIcon {
                                Layout.alignment: Qt.AlignHCenter
                                iconPath: Weather.fetchStatus === "loading" || Weather.fetchStatus === "refreshing"
                                          ? "lucide/loader-circle.svg"
                                          : "lucide/circle-alert.svg"
                                size: Theme.clockWeatherIconSize
                                color: root._weatherStatusColor

                                RotationAnimator on rotation {
                                    loops: Animation.Infinite
                                    from: 0
                                    to: 360
                                    duration: Theme.motionDeliberate * 4
                                    running: Weather.fetchStatus === "loading" || Weather.fetchStatus === "refreshing"
                                }
                            }

                            Text {
                                Layout.alignment: Qt.AlignHCenter
                                text: Weather.fetchStatus === "loading" || Weather.fetchStatus === "refreshing"
                                      ? "Loading weather..."
                                      : "Weather unavailable"
                                color: Theme.fgPrimary
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontLabel
                                font.weight: Theme.weightMedium
                            }

                            Text {
                                Layout.alignment: Qt.AlignHCenter
                                width: weatherCard.width - Theme.spaceXxl * 2
                                wrapMode: Text.WordWrap
                                horizontalAlignment: Text.AlignHCenter
                                text: Weather.errorInfo && Weather.errorInfo.message
                                      ? Weather.errorInfo.message
                                      : "Refresh to fetch a new weather snapshot."
                                color: Theme.fgMuted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.weight: Theme.weightRegular
                            }
                        }
                    }
                }
            }
        }
    }
}
