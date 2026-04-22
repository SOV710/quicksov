// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "WallpaperContract.hpp"

#include <optional>
#include <vector>

#include <sys/types.h>

#include <QHash>
#include <QImage>
#include <QJsonObject>
#include <QJsonValue>
#include <QPainter>
#include <QRectF>
#include <QSize>
#include <QStringList>

extern "C" {
#include <drm_fourcc.h>
#include <libavutil/frame.h>
#include <wayland-client.h>
}

namespace quicksov::wallpaper::renderer {

inline constexpr const char *kLogPrefix = "[wallpaper-renderer]";
inline constexpr const char *kNamespace = shared::kLayerNamespace;
inline constexpr int kTransitionFrameMs = 16;
inline constexpr uint32_t kDmabufDrmFormat = DRM_FORMAT_ARGB8888;

struct CropRect {
    double x = 0.0;
    double y = 0.0;
    double width = 1.0;
    double height = 1.0;

    auto operator==(const CropRect &) const -> bool = default;
};

struct SourceSpec {
    QString id;
    QString path;
    QString name;
    QString kind;
    bool loopEnabled = true;
    bool mute = true;
};

struct ViewSpec {
    QString output;
    QString source;
    QString fit = QStringLiteral("cover");
    std::optional<CropRect> crop;
};

struct SnapshotModel {
    QHash<QString, SourceSpec> sources;
    QHash<QString, ViewSpec> views;
    QString fallbackSource;
    QStringList decodeBackendOrder;
    QString decodeDevicePolicy = QStringLiteral("same-as-render");
    QString renderDevicePolicy = QStringLiteral("same-as-compositor");
    bool allowCrossGpu = false;
    QString presentBackend = QStringLiteral("auto");
    int transitionDurationMs = 0;
};

enum class GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
};

struct GpuDeviceInfo {
    QString nodePath;
    QString sysfsDevicePath;
    QString vendorId;
    QString deviceId;
    GpuVendor vendor = GpuVendor::Other;
    bool discrete = false;
    dev_t deviceNumber = 0;
};

struct DmabufFormatModifier {
    uint32_t format = 0;
    uint64_t modifier = DRM_FORMAT_MOD_INVALID;

    auto operator==(const DmabufFormatModifier &) const -> bool = default;
};

struct DmabufTranche {
    std::optional<dev_t> targetDevice;
    uint32_t flags = 0;
    std::vector<DmabufFormatModifier> formats;
};

QString defaultSocketPath();
QString normalizePresentBackend(QString backend);
QString normalizeGpuPolicy(QString policy, const QString &fallback);
QString drmFormatString(uint32_t format);
QString dmabufModifierString(uint64_t modifier);
QString gpuVendorString(GpuVendor vendor);
QVector<GpuDeviceInfo> discoverGpuDevices();
QString describeGpuDevices(const QVector<GpuDeviceInfo> &devices);
QString selectGpuDevicePath(
    const QVector<GpuDeviceInfo> &devices,
    const QString &policy,
    const QString &preferredPath
);
std::optional<GpuDeviceInfo> gpuInfoForPath(
    const QVector<GpuDeviceInfo> &devices,
    const QString &path
);
QStringList reorderDecodeBackendsForGpu(QStringList backends, GpuVendor vendor);
QString describeAvFrame(const AVFrame *frame);
QString describeDerivedDrmFrame(const AVFrame *frame);
uint32_t modifierHi(uint64_t modifier);
uint32_t modifierLo(uint64_t modifier);
std::optional<dev_t> parseDeviceNumber(const wl_array *array);
QString firstExistingDrmNode(const QStringList &patterns);
QString drmNodePathForDevice(dev_t device);
QRectF cropRectFor(const QSize &sourceSize, const std::optional<CropRect> &crop);
QRectF coverSourceRect(const QRectF &sourceRect, const QSize &targetSize);
void paintImageCover(
    QPainter &painter,
    const QImage &image,
    const QSize &targetSize,
    const std::optional<CropRect> &crop,
    qreal opacity
);
std::optional<CropRect> parseCrop(const QJsonValue &value);
SnapshotModel parseSnapshot(const QJsonObject &payload);

} // namespace quicksov::wallpaper::renderer
