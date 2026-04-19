// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick
import Quickshell
import Quicksov.WallpaperFfmpeg 1.0
import "./"

Singleton {
    id: root

    property var _controllers: ({})

    function _fileUrl(path) {
        if (!path)
            return "";
        return "file://" + String(path).split("/").map(function(segment) {
            return encodeURIComponent(segment);
        }).join("/");
    }

    function _debugName(outputName, sourceData) {
        var sourceId = sourceData && sourceData.id ? String(sourceData.id) : "none";
        return String(outputName) + "::" + sourceId;
    }

    function _syncController(outputName) {
        if (!outputName)
            return null;

        var key = String(outputName);
        var controller = root._controllers[key];
        var source = Wallpaper.sourceForOutput(key);

        if (!source || source.kind !== "video") {
            if (controller) {
                controller.source = "";
            }
            return controller || null;
        }

        if (!controller) {
            controller = controllerComponent.createObject(root, {
                "debugName": root._debugName(key, source)
            });
            root._controllers[key] = controller;
        }

        controller.debugName = root._debugName(key, source);
        controller.muted = !Wallpaper.videoAudio || !!source.mute;
        controller.ensureInitialized();
        controller.source = root._fileUrl(source.path);
        return controller;
    }

    function controllerForOutput(outputName) {
        return root._syncController(outputName);
    }

    function releaseOutput(outputName) {
        if (!outputName)
            return;
        var key = String(outputName);
        var controller = root._controllers[key];
        if (!controller)
            return;
        controller.destroy();
        delete root._controllers[key];
    }

    function sync() {
        var outputIds = Object.keys(root._controllers);
        for (var i = 0; i < outputIds.length; ++i)
            root._syncController(outputIds[i]);
    }

    Component {
        id: controllerComponent

        WallpaperVideo {}
    }

    Connections {
        target: Wallpaper

        function onSourcesChanged() {
            root.sync();
        }

        function onViewsChanged() {
            root.sync();
        }

        function onFallbackSourceChanged() {
            root.sync();
        }

        function onVideoAudioChanged() {
            root.sync();
        }
    }
}
