// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quicksov.WallpaperMpv 1.0
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

            // Keep the background fully click-through.
            mask: Region {
                item: Item {}
            }

            Item {
                id: root
                anchors.fill: parent
                clip: true

                WallpaperVideo {
                    id: videoController
                    debugName: wallpaperWindow.screen && wallpaperWindow.screen.name
                               ? wallpaperWindow.screen.name
                               : "unknown"
                }

                property string liveKind: ""
                property string liveSource: ""
                property string overlaySource: ""
                property real overlayOpacity: 0
                property bool transitionPending: false
                property int switchToken: 0

                readonly property string targetKind: Wallpaper.hasRenderableImage
                                                     ? "image"
                                                     : (Wallpaper.hasRenderableVideo ? "video" : "")
                readonly property string targetSource: root.targetKind !== ""
                                                       && Wallpaper.hasCurrentEntry
                                                       ? root.fileUrl(Wallpaper.currentPath)
                                                       : ""
                readonly property bool contentReady: liveSource === ""
                                                     || (liveLoader.item && liveLoader.item.contentReady === true)
                readonly property bool contentError: liveLoader.item && liveLoader.item.contentError === true
                readonly property real dpr: wallpaperWindow.screen && wallpaperWindow.screen.devicePixelRatio
                                            ? wallpaperWindow.screen.devicePixelRatio
                                            : 1
                readonly property size textureSize: Qt.size(
                    Math.max(1, Math.round((wallpaperWindow.screen ? wallpaperWindow.screen.width : 1920) * dpr)),
                    Math.max(1, Math.round((wallpaperWindow.screen ? wallpaperWindow.screen.height : 1080) * dpr))
                )

                function fileUrl(path) {
                    if (!path)
                        return "";
                    return "file://" + String(path).split("/").map(function(segment) {
                        return encodeURIComponent(segment);
                    }).join("/");
                }

                function clearOverlay() {
                    overlayFade.stop();
                    root.overlaySource = "";
                    root.overlayOpacity = 0;
                    root.transitionPending = false;
                }

                function applyLiveContent(kind, source) {
                    var resolvedSource = kind === "" ? "" : source;
                    console.log("[wallpaper-layer] screen="
                                + (wallpaperWindow.screen && wallpaperWindow.screen.name
                                   ? wallpaperWindow.screen.name : "unknown")
                                + " apply kind=" + kind + " source=" + resolvedSource);
                    root.liveKind = kind;
                    root.liveSource = resolvedSource;
                    if (kind === "video") {
                        videoController.muted = !Wallpaper.videoAudio;
                        videoController.ensureInitialized();
                        videoController.source = resolvedSource;
                    } else {
                        videoController.source = "";
                    }
                }

                function finishTransition() {
                    if (!root.transitionPending)
                        return;
                    if (Wallpaper.transitionDurationMs <= 0 || root.overlaySource === "") {
                        root.clearOverlay();
                        return;
                    }
                    overlayFade.restart();
                }

                function syncTarget() {
                    var nextKind = root.targetKind;
                    var nextSource = root.targetSource;
                    console.log("[wallpaper-layer] screen="
                                + (wallpaperWindow.screen && wallpaperWindow.screen.name
                                   ? wallpaperWindow.screen.name : "unknown")
                                + " target kind=" + nextKind + " source=" + nextSource);

                    if (nextKind !== "" && nextSource === "")
                        return;

                    if (nextKind === root.liveKind && nextSource === root.liveSource)
                        return;

                    root.switchToken += 1;
                    var token = root.switchToken;

                    if (root.liveSource === "" || Wallpaper.transitionDurationMs <= 0) {
                        root.clearOverlay();
                        root.applyLiveContent(nextKind, nextSource);
                        return;
                    }

                    root.grabToImage(function(result) {
                        if (token !== root.switchToken)
                            return;

                        root.overlaySource = result && result.url ? result.url : "";
                        root.overlayOpacity = root.overlaySource !== "" ? 1 : 0;
                        root.transitionPending = root.overlaySource !== "";
                        root.applyLiveContent(nextKind, nextSource);

                        if (!root.transitionPending || root.contentReady || root.contentError)
                            root.finishTransition();
                        else
                            overlayFailSafe.restart();
                    });
                }

                onTargetKindChanged: syncTarget()
                onTargetSourceChanged: syncTarget()
                onContentReadyChanged: {
                    console.log("[wallpaper-layer] screen="
                                + (wallpaperWindow.screen && wallpaperWindow.screen.name
                                   ? wallpaperWindow.screen.name : "unknown")
                                + " contentReady=" + contentReady
                                + " contentError=" + contentError
                                + " liveKind=" + liveKind);
                    if (contentReady || contentError)
                        root.finishTransition();
                }
                onContentErrorChanged: {
                    console.log("[wallpaper-layer] screen="
                                + (wallpaperWindow.screen && wallpaperWindow.screen.name
                                   ? wallpaperWindow.screen.name : "unknown")
                                + " contentError changed to " + contentError
                                + " liveKind=" + liveKind);
                    if (contentError)
                        root.finishTransition();
                }

                Loader {
                    id: liveLoader
                    anchors.fill: parent
                    active: root.liveKind !== ""
                    sourceComponent: root.liveKind === "image"
                                     ? imageWallpaperComponent
                                     : (root.liveKind === "video" ? videoWallpaperComponent : null)
                }

                Image {
                    id: overlaySourceImage
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

                    onFinished: {
                        root.clearOverlay();
                    }
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
                        property bool contentReady: image.status === Image.Ready || root.liveSource === ""
                        property bool contentError: image.status === Image.Error

                        Image {
                            id: image
                            anchors.fill: parent
                            source: root.liveSource
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
                        property bool contentReady: videoItem.ready
                        property bool contentError: videoController.status === "error"

                        WallpaperVideoItem {
                            id: videoItem
                            anchors.fill: parent
                            controller: videoController
                        }
                    }
                }

                Connections {
                    target: Wallpaper
                    function onVideoAudioChanged() {
                        videoController.muted = !Wallpaper.videoAudio;
                    }
                }

                Component.onCompleted: syncTarget()
            }
        }
    }
}
