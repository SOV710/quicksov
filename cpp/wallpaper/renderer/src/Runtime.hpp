// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "ProtocolClient.hpp"
#include "WaylandRenderer.hpp"

#include <QObject>

namespace quicksov::wallpaper::renderer {

class WallpaperRuntime final : public QObject {
    Q_OBJECT

public:
    explicit WallpaperRuntime(QObject *parent = nullptr);

    int start();

private:
    void fail(const QString &message);

    WaylandRenderer m_renderer;
    WallpaperProtocolClient m_protocol;
};

} // namespace quicksov::wallpaper::renderer
