// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Item {
    id: root

    property var geometry: null
    property Component contentComponent: null
    property Component _retainedContentComponent: null

    readonly property real contentImplicitHeight: contentLoader.item ? contentLoader.item.implicitHeight : 0

    x: geometry ? geometry.contentX : 0
    y: geometry ? geometry.contentY : 0
    width: geometry ? geometry.contentWidth : 0
    height: geometry ? geometry.contentHeight : 0
    visible: geometry ? geometry.active : false
    clip: true

    onContentComponentChanged: {
        if (contentComponent)
            _retainedContentComponent = contentComponent;
    }

    Loader {
        id: contentLoader
        anchors.fill: parent
        active: root.geometry ? (root.geometry.open || root.geometry.active) : false
        sourceComponent: root.contentComponent ? root.contentComponent : root._retainedContentComponent
    }
}
