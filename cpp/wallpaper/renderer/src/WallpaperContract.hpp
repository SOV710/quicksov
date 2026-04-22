// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "WallpaperContractConfig.hpp"

#include <QStringList>
#include <QStringView>

namespace quicksov::wallpaper::shared {

inline bool isSupportedDecodeBackend(QStringView backend) {
    for (const char *candidate : kDecodeBackendCatalog) {
        if (backend == QLatin1String(candidate)) {
            return true;
        }
    }
    return false;
}

inline QStringList defaultDecodeBackendOrder() {
    QStringList normalized;
    normalized.reserve(static_cast<qsizetype>(kDefaultDecodeBackendOrder.size()));
    for (const char *backend : kDefaultDecodeBackendOrder) {
        normalized.push_back(QString::fromLatin1(backend));
    }
    return normalized;
}

inline QStringList normalizeDecodeBackendOrder(QStringList order) {
    QStringList normalized;
    normalized.reserve(order.size() + 1);

    for (QString &entry : order) {
        entry = entry.trimmed().toLower();
        if (entry.isEmpty()) {
            continue;
        }
        if (!isSupportedDecodeBackend(QStringView(entry))) {
            continue;
        }
        if (!normalized.contains(entry)) {
            normalized.push_back(entry);
        }
    }

    const QString softwareBackend = QString::fromLatin1(kSoftwareDecodeBackend);
    if (!normalized.contains(softwareBackend)) {
        normalized.push_back(softwareBackend);
    }

    return normalized;
}

} // namespace quicksov::wallpaper::shared
