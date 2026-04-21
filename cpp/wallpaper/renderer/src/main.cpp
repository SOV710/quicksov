// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "Runtime.hpp"

#include <QCoreApplication>

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("qsov-wallpaper-renderer"));

    quicksov::wallpaper::renderer::WallpaperRuntime runtime;
    const int startup = runtime.start();
    if (startup != 0) {
        return startup;
    }

    return app.exec();
}
