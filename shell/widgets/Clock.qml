// SPDX-FileCopyrightText: 2026 SOV710
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Item {
    id: root

    implicitWidth: label.implicitWidth
    implicitHeight: label.implicitHeight

    signal openPopup()

    // Format: "2026-04-12 · 19:38 CST · Sun"
    property string _clockText: _formatClock(new Date())

    function _tzAbbr(d) {
        // Extract timezone abbreviation from locale time string (e.g. "19:38:00 CST")
        var s = d.toLocaleTimeString(Qt.locale(), "t");
        return s || "";
    }

    function _formatClock(d) {
        var date    = Qt.formatDate(d, "yyyy-MM-dd");
        var time    = Qt.formatTime(d, "HH:mm");
        var tz      = root._tzAbbr(d);
        var weekday = Qt.formatDate(d, "ddd");
        if (tz) return date + " · " + time + " " + tz + " · " + weekday;
        return date + " · " + time + " · " + weekday;
    }

    Timer {
        interval: 1000
        running: true
        repeat: true
        onTriggered: root._clockText = root._formatClock(new Date())
    }

    Text {
        id: label
        text: root._clockText
        color: Theme.fgPrimary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.weight: Theme.weightRegular
        font.features: { "tnum": 1 }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.openPopup()
    }
}
