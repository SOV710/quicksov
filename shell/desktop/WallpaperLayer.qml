// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import Quickshell
import Quickshell.Wayland
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

                property string activeSource: ""
                property string stagedSource: ""
                property real transitionProgress: 1
                property bool stageReady: false

                readonly property string targetSource: Wallpaper.hasRenderableImage
                                                      ? root.fileUrl(Wallpaper.currentPath)
                                                      : ""
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

                function syncTarget() {
                    var target = root.targetSource;

                    if (crossfade.running)
                        crossfade.stop();

                    if (target === "") {
                        root.activeSource = "";
                        root.stagedSource = "";
                        root.stageReady = false;
                        root.transitionProgress = 1;
                        return;
                    }

                    if (target === root.activeSource && root.stagedSource === "")
                        return;

                    if (root.activeSource === "" || Wallpaper.transitionDurationMs <= 0) {
                        root.activeSource = target;
                        root.stagedSource = "";
                        root.stageReady = false;
                        root.transitionProgress = 1;
                        return;
                    }

                    root.stagedSource = target;
                    root.stageReady = stagedImage.status === Image.Ready;
                    root.transitionProgress = 0;

                    if (root.stageReady)
                        root.startCrossfade();
                }

                function startCrossfade() {
                    if (root.stagedSource === "" || !root.stageReady)
                        return;

                    if (root.activeSource === "" || Wallpaper.transitionDurationMs <= 0) {
                        root.activeSource = root.stagedSource;
                        root.stagedSource = "";
                        root.stageReady = false;
                        root.transitionProgress = 1;
                        return;
                    }

                    root.transitionProgress = 0;
                    crossfade.start();
                }

                onTargetSourceChanged: syncTarget()

                Image {
                    id: activeImage
                    anchors.fill: parent
                    source: root.activeSource
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    cache: true
                    smooth: true
                    mipmap: true
                    sourceSize: root.textureSize
                    opacity: root.stagedSource !== ""
                             ? (1 - root.transitionProgress)
                             : (root.activeSource !== "" ? 1 : 0)
                }

                Image {
                    id: stagedImage
                    anchors.fill: parent
                    source: root.stagedSource
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    cache: true
                    smooth: true
                    mipmap: true
                    sourceSize: root.textureSize
                    opacity: root.stagedSource !== "" ? root.transitionProgress : 0

                    onStatusChanged: {
                        if (root.stagedSource === "")
                            return;

                        if (status === Image.Ready) {
                            root.stageReady = true;
                            root.startCrossfade();
                        } else if (status === Image.Error) {
                            console.warn("[wallpaper] failed to load:", root.stagedSource);
                            root.stagedSource = "";
                            root.stageReady = false;
                            root.transitionProgress = 1;
                        }
                    }
                }

                NumberAnimation {
                    id: crossfade
                    target: root
                    property: "transitionProgress"
                    from: 0
                    to: 1
                    duration: Math.max(0, Wallpaper.transitionDurationMs)
                    easing.type: Easing.OutCubic

                    onFinished: {
                        if (root.stagedSource === "")
                            return;

                        root.activeSource = root.stagedSource;
                        root.stagedSource = "";
                        root.stageReady = false;
                        root.transitionProgress = 1;
                    }
                }

                Component.onCompleted: syncTarget()
            }
        }
    }
}
