// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import ".."

Item {
    id: root

    property string surfaceName: "panel"
    property var geometry: null
    property Component contentComponent: null
    property Component _retainedContentComponent: null

    readonly property bool contentRevealReady: contentLoader.item && contentLoader.item.revealReady !== undefined
                                               ? contentLoader.item.revealReady
                                               : true
    readonly property real rawContentImplicitHeight: contentLoader.item ? contentLoader.item.implicitHeight : 0
    readonly property real contentImplicitHeight: contentRevealReady ? rawContentImplicitHeight : 0
    readonly property real finalContentHeight: geometry ? Math.min(contentImplicitHeight, geometry.maxBodyHeight) : 0
    readonly property string transitionPhase: geometry
                                              ? (geometry.open ? "popup-open" : (geometry.active ? "popup-close" : "popup-idle"))
                                              : "popup-idle"

    x: geometry ? geometry.contentX : 0
    y: geometry ? geometry.contentY : 0
    width: geometry ? geometry.contentWidth : 0
    height: DebugVisuals.freezePanelBodyHeightToFinal ? finalContentHeight : (geometry ? geometry.contentHeight : 0)
    visible: geometry ? geometry.active : false
    opacity: geometry && geometry.contentVisible ? 1 : 0
    clip: !DebugVisuals.disablePanelContentClip

    Behavior on opacity {
        NumberAnimation {
            duration: DebugVisuals.duration(Theme.motionFast)
        }
    }

    onContentComponentChanged: {
        if (contentComponent)
            _retainedContentComponent = contentComponent;
    }
    onContentImplicitHeightChanged: {
        DebugVisuals.logTransition(root.surfaceName, root.transitionPhase, {
            clip: root.clip,
            contentImplicitHeight: root.contentImplicitHeight,
            contentRevealReady: root.contentRevealReady,
            event: "content-implicit-height-changed",
            finalContentHeight: root.finalContentHeight,
            geometryContentHeight: root.geometry ? root.geometry.contentHeight : 0,
            loaderImplicitHeight: root.rawContentImplicitHeight,
            slotHeight: root.height
        });
    }
    onContentRevealReadyChanged: {
        DebugVisuals.logTransition(root.surfaceName, root.transitionPhase, {
            clip: root.clip,
            contentImplicitHeight: root.contentImplicitHeight,
            contentRevealReady: root.contentRevealReady,
            event: "content-reveal-ready-changed",
            finalContentHeight: root.finalContentHeight,
            geometryContentHeight: root.geometry ? root.geometry.contentHeight : 0,
            loaderImplicitHeight: root.rawContentImplicitHeight,
            slotHeight: root.height
        });
    }
    onHeightChanged: {
        DebugVisuals.logTransition(root.surfaceName, root.transitionPhase, {
            clip: root.clip,
            contentImplicitHeight: root.contentImplicitHeight,
            contentRevealReady: root.contentRevealReady,
            event: "slot-height-changed",
            finalContentHeight: root.finalContentHeight,
            geometryContentHeight: root.geometry ? root.geometry.contentHeight : 0,
            loaderImplicitHeight: root.rawContentImplicitHeight,
            opacity: root.opacity,
            slotHeight: root.height
        });
    }

    Loader {
        id: contentLoader
        anchors.fill: parent
        active: root.geometry ? (root.geometry.open || root.geometry.active) : false
        sourceComponent: root.contentComponent ? root.contentComponent : root._retainedContentComponent

        onItemChanged: {
            DebugVisuals.logTransition(root.surfaceName, root.transitionPhase, {
                clip: root.clip,
                contentImplicitHeight: root.contentImplicitHeight,
                contentRevealReady: root.contentRevealReady,
                event: item ? "loader-item-attached" : "loader-item-cleared",
                finalContentHeight: root.finalContentHeight,
                geometryContentHeight: root.geometry ? root.geometry.contentHeight : 0,
                loaderImplicitHeight: root.rawContentImplicitHeight,
                slotHeight: root.height
            });
        }
    }
}
