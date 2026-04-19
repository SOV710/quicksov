// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quicksov.WallpaperFfmpeg 1.0
import ".."
import "../services"

Scope {
    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: wallpaperWindow

            required property var modelData

            screen: modelData

            WlrLayershell.layer: WlrLayer.Background
            WlrLayershell.namespace: "quicksov-wallpaper-" + (screen && screen.name ? screen.name : "unknown")
            WlrLayershell.exclusionMode: ExclusionMode.Ignore

            anchors.top: true
            anchors.bottom: true
            anchors.left: true
            anchors.right: true

            color: Theme.bgCanvas
            visible: true

            mask: Region {
                item: Item {}
            }

            Item {
                id: root
                anchors.fill: parent
                clip: true

                readonly property string screenName: wallpaperWindow.screen && wallpaperWindow.screen.name
                                                     ? wallpaperWindow.screen.name
                                                     : ""
                readonly property var targetSourceData: Wallpaper.sourceForOutput(screenName)
                readonly property string targetSourceId: targetSourceData && typeof targetSourceData.id === "string"
                                                         ? targetSourceData.id
                                                         : ""
                readonly property string targetKind: targetSourceData && typeof targetSourceData.kind === "string"
                                                     ? targetSourceData.kind
                                                     : ""
                readonly property string targetPath: targetSourceData && typeof targetSourceData.path === "string"
                                                     ? targetSourceData.path
                                                     : ""
                readonly property var targetCrop: Wallpaper.cropForOutput(screenName)
                readonly property real dpr: wallpaperWindow.screen && wallpaperWindow.screen.devicePixelRatio
                                            ? wallpaperWindow.screen.devicePixelRatio
                                            : 1
                readonly property size textureSize: Qt.size(
                    Math.max(1, Math.round((wallpaperWindow.screen ? wallpaperWindow.screen.width : 1920) * dpr)),
                    Math.max(1, Math.round((wallpaperWindow.screen ? wallpaperWindow.screen.height : 1080) * dpr))
                )
                readonly property bool contentReady: liveLoader.item && liveLoader.item.contentReady === true
                readonly property bool contentError: liveLoader.item && liveLoader.item.contentError === true

                property var liveSourceData: null
                property var liveCrop: null
                property string overlaySource: ""
                property real overlayOpacity: 0
                property bool transitionPending: false
                property int switchToken: 0

                function fileUrl(path) {
                    if (!path)
                        return "";
                    return "file://" + String(path).split("/").map(function(segment) {
                        return encodeURIComponent(segment);
                    }).join("/");
                }

                function cropRect(crop) {
                    if (!crop)
                        return Qt.rect(0, 0, 0, 0);
                    return Qt.rect(crop.x || 0, crop.y || 0, crop.width || 0, crop.height || 0);
                }

                function sameCrop(left, right) {
                    if (!left && !right)
                        return true;
                    if (!left || !right)
                        return false;
                    return left.x === right.x
                        && left.y === right.y
                        && left.width === right.width
                        && left.height === right.height;
                }

                function clearOverlay() {
                    overlayFade.stop();
                    overlaySource = "";
                    overlayOpacity = 0;
                    transitionPending = false;
                }

                function applyLiveContent(sourceData, crop) {
                    liveSourceData = sourceData || null;
                    liveCrop = crop || null;
                    if (sourceData && sourceData.kind === "video")
                        WallpaperSessions.controllerForOutput(screenName);
                }

                function finishTransition() {
                    if (!transitionPending)
                        return;
                    if (Wallpaper.transitionDurationMs <= 0 || overlaySource === "") {
                        clearOverlay();
                        return;
                    }
                    overlayFade.restart();
                }

                function syncTarget() {
                    var nextSource = targetSourceData;
                    var sameSource = !!liveSourceData
                        && !!nextSource
                        && liveSourceData.id === nextSource.id
                        && liveSourceData.path === nextSource.path
                        && liveSourceData.kind === nextSource.kind
                        && sameCrop(liveCrop, targetCrop);

                    if (!liveSourceData && !nextSource)
                        return;
                    if (sameSource)
                        return;

                    switchToken += 1;
                    var token = switchToken;

                    if (!liveSourceData || Wallpaper.transitionDurationMs <= 0) {
                        clearOverlay();
                        applyLiveContent(nextSource, targetCrop);
                        return;
                    }

                    root.grabToImage(function(result) {
                        if (token !== switchToken)
                            return;

                        overlaySource = result && result.url ? result.url : "";
                        overlayOpacity = overlaySource !== "" ? 1 : 0;
                        transitionPending = overlaySource !== "";
                        applyLiveContent(nextSource, targetCrop);

                        if (!transitionPending || contentReady || contentError)
                            finishTransition();
                        else
                            overlayFailSafe.restart();
                    });
                }

                onTargetSourceIdChanged: syncTarget()
                onTargetKindChanged: syncTarget()
                onTargetPathChanged: syncTarget()
                onTargetCropChanged: syncTarget()
                onContentReadyChanged: {
                    if (contentReady || contentError)
                        finishTransition();
                }
                onContentErrorChanged: {
                    if (contentError)
                        finishTransition();
                }

                Rectangle {
                    anchors.fill: parent
                    color: Theme.bgCanvas
                    visible: !liveLoader.active
                }

                Loader {
                    id: liveLoader
                    anchors.fill: parent
                    active: !!root.liveSourceData
                    sourceComponent: root.liveSourceData && root.liveSourceData.kind === "image"
                                     ? imageWallpaperComponent
                                     : (root.liveSourceData && root.liveSourceData.kind === "video"
                                        ? videoWallpaperComponent
                                        : null)
                }

                Image {
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    cache: true
                    smooth: true
                    mipmap: true
                    sourceSize: root.textureSize
                    source: root.overlaySource
                    opacity: root.overlayOpacity
                    visible: opacity > 0
                }

                NumberAnimation {
                    id: overlayFade
                    target: root
                    property: "overlayOpacity"
                    from: root.overlayOpacity
                    to: 0
                    duration: Math.max(0, Wallpaper.transitionDurationMs)
                    easing.type: Easing.OutCubic

                    onFinished: root.clearOverlay()
                }

                Timer {
                    id: overlayFailSafe
                    interval: Math.max(1000, Wallpaper.transitionDurationMs * 4)
                    repeat: false
                    onTriggered: root.finishTransition()
                }

                Component {
                    id: imageWallpaperComponent

                    Item {
                        property bool contentReady: image.status === Image.Ready || !root.liveSourceData
                        property bool contentError: image.status === Image.Error

                        Image {
                            id: image
                            anchors.fill: parent
                            source: root.liveSourceData ? root.fileUrl(root.liveSourceData.path) : ""
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            cache: true
                            smooth: true
                            mipmap: true
                            sourceSize: root.textureSize
                        }
                    }
                }

                Component {
                    id: videoWallpaperComponent

                    Item {
                        readonly property var controller: root.liveSourceData
                                                          ? WallpaperSessions.controllerForOutput(root.screenName)
                                                          : null
                        property bool contentReady: controller ? controller.ready : false
                        property bool contentError: controller ? controller.status === "error" : false

                        WallpaperVideoItem {
                            anchors.fill: parent
                            controller: parent.controller
                            cropRect: root.cropRect(root.liveCrop)
                        }
                    }
                }

                Component.onCompleted: syncTarget()
                Component.onDestruction: WallpaperSessions.releaseOutput(screenName)
            }
        }
    }
}
