// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "Runtime.hpp"
#include "WallpaperContract.hpp"

#include <QCoreApplication>

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    app.setApplicationName(
        QString::fromLatin1(quicksov::wallpaper::shared::kRendererClientName)
    );
    app.setApplicationVersion(
        QString::fromLatin1(quicksov::wallpaper::shared::kRendererClientVersion)
    );

    quicksov::wallpaper::renderer::WallpaperRuntime runtime;
    const int startup = runtime.start();
    if (startup != 0) {
        return startup;
    }

    return app.exec();
}
