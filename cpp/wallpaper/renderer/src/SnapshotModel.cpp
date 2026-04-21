// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "SnapshotModel.hpp"

#include <algorithm>
#include <cstring>
#include <limits>

#include <sys/stat.h>
#include <unistd.h>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>

extern "C" {
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_drm.h>
#include <libavutil/pixdesc.h>
}

namespace quicksov::wallpaper::renderer {

namespace {

double clamp01(double value) {
    return std::clamp(value, 0.0, 1.0);
}

QRectF clampRectToBounds(const QRectF &rect, const QRectF &bounds) {
    if (!bounds.isValid()) {
        return QRectF();
    }

    const double width = std::clamp(rect.width(), 1.0, bounds.width());
    const double height = std::clamp(rect.height(), 1.0, bounds.height());
    const double left = std::clamp(rect.left(), bounds.left(), bounds.right() - width);
    const double top = std::clamp(rect.top(), bounds.top(), bounds.bottom() - height);
    return QRectF(left, top, width, height);
}

GpuVendor gpuVendorFromId(const QString &vendorId) {
    const QString normalized = vendorId.trimmed().toLower();
    if (normalized == QLatin1String("0x10de")) {
        return GpuVendor::Nvidia;
    }
    if (normalized == QLatin1String("0x1002") || normalized == QLatin1String("0x1022")) {
        return GpuVendor::Amd;
    }
    if (normalized == QLatin1String("0x8086")) {
        return GpuVendor::Intel;
    }
    return GpuVendor::Other;
}

int gpuVendorRank(GpuVendor vendor) {
    switch (vendor) {
    case GpuVendor::Nvidia:
        return 300;
    case GpuVendor::Amd:
        return 200;
    case GpuVendor::Intel:
        return 100;
    case GpuVendor::Other:
    default:
        return 0;
    }
}

QString readTrimmedFile(const QString &path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        return QString();
    }
    return QString::fromUtf8(file.readAll()).trimmed();
}

bool isDiscretePciDevicePath(const QString &sysfsDevicePath) {
    const QStringList segments = sysfsDevicePath.split(QLatin1Char('/'), Qt::SkipEmptyParts);
    int pciSegments = 0;
    for (const QString &segment : segments) {
        if (segment.contains(QLatin1Char(':')) && segment.contains(QLatin1Char('.'))) {
            pciSegments += 1;
        }
    }
    return pciSegments > 1;
}

std::optional<dev_t> deviceNumberForPath(const QString &path) {
    struct stat st {};
    if (::stat(path.toUtf8().constData(), &st) != 0) {
        return std::nullopt;
    }
    if (!S_ISCHR(st.st_mode)) {
        return std::nullopt;
    }
    return st.st_rdev;
}

int scoreGpuForPolicy(
    const GpuDeviceInfo &device,
    const QString &policy,
    const QString &preferredPath
) {
    int score = gpuVendorRank(device.vendor);
    if (policy == QLatin1String("auto") &&
        !preferredPath.isEmpty() &&
        device.nodePath == preferredPath) {
        score += 10'000;
    } else if (!preferredPath.isEmpty() && device.nodePath == preferredPath) {
        score += 10;
    }

    return score;
}

bool gpuMatchesPolicy(const GpuDeviceInfo &device, const QString &policy) {
    if (policy == QLatin1String("nvidia")) {
        return device.vendor == GpuVendor::Nvidia;
    }
    if (policy == QLatin1String("amdgpu")) {
        return device.vendor == GpuVendor::Amd;
    }
    if (policy == QLatin1String("intel")) {
        return device.vendor == GpuVendor::Intel;
    }
    if (policy == QLatin1String("prefer-discrete")) {
        return device.discrete;
    }
    if (policy == QLatin1String("prefer-integrated")) {
        return !device.discrete;
    }
    return true;
}

QString avPixelFormatString(int format) {
    const char *name = av_get_pix_fmt_name(static_cast<AVPixelFormat>(format));
    if (name == nullptr) {
        return QStringLiteral("unknown(%1)").arg(format);
    }
    return QString::fromUtf8(name);
}

} // namespace

QString defaultSocketPath() {
    const QByteArray env = qgetenv("QSOV_SOCKET");
    if (!env.isEmpty()) {
        return QString::fromUtf8(env);
    }

    const QByteArray runtimeDir = qgetenv("XDG_RUNTIME_DIR");
    if (!runtimeDir.isEmpty()) {
        return QString::fromUtf8(runtimeDir) + QStringLiteral("/quicksov/daemon.sock");
    }

    return QStringLiteral("/run/user/%1/quicksov/daemon.sock").arg(::getuid());
}

QString normalizePresentBackend(QString backend) {
    backend = backend.trimmed().toLower();
    if (backend == QLatin1String("shm") || backend == QLatin1String("dmabuf")) {
        return backend;
    }
    return QStringLiteral("auto");
}

QString normalizeGpuPolicy(QString policy, const QString &fallback) {
    policy = policy.trimmed().toLower();
    if (policy == QLatin1String("auto") ||
        policy == QLatin1String("same-as-compositor") ||
        policy == QLatin1String("same-as-render") ||
        policy == QLatin1String("prefer-discrete") ||
        policy == QLatin1String("prefer-integrated") ||
        policy == QLatin1String("nvidia") ||
        policy == QLatin1String("amdgpu") ||
        policy == QLatin1String("intel")) {
        return policy;
    }
    return fallback;
}

QString drmFormatString(uint32_t format) {
    QByteArray text(4, '\0');
    text[0] = static_cast<char>(format & 0xff);
    text[1] = static_cast<char>((format >> 8) & 0xff);
    text[2] = static_cast<char>((format >> 16) & 0xff);
    text[3] = static_cast<char>((format >> 24) & 0xff);
    return QString::fromLatin1(text);
}

QString dmabufModifierString(uint64_t modifier) {
    if (modifier == DRM_FORMAT_MOD_INVALID) {
        return QStringLiteral("invalid");
    }
    if (modifier == DRM_FORMAT_MOD_LINEAR) {
        return QStringLiteral("linear");
    }
    return QStringLiteral("0x%1").arg(QString::number(modifier, 16));
}

QString gpuVendorString(GpuVendor vendor) {
    switch (vendor) {
    case GpuVendor::Nvidia:
        return QStringLiteral("nvidia");
    case GpuVendor::Amd:
        return QStringLiteral("amdgpu");
    case GpuVendor::Intel:
        return QStringLiteral("intel");
    case GpuVendor::Other:
    default:
        return QStringLiteral("other");
    }
}

QVector<GpuDeviceInfo> discoverGpuDevices() {
    QVector<GpuDeviceInfo> devices;
    const QDir drmDir(QStringLiteral("/sys/class/drm"));
    const QStringList entries =
        drmDir.entryList(QStringList{QStringLiteral("renderD*")},
                         QDir::AllEntries | QDir::NoDotAndDotDot,
                         QDir::Name);

    for (const QString &entry : entries) {
        const QString nodePath = QStringLiteral("/dev/dri/%1").arg(entry);
        const QString sysfsDevicePath =
            QFileInfo(drmDir.absoluteFilePath(entry) + QStringLiteral("/device")).canonicalFilePath();
        if (sysfsDevicePath.isEmpty()) {
            continue;
        }

        const QString vendorId = readTrimmedFile(sysfsDevicePath + QStringLiteral("/vendor"));
        const QString deviceId = readTrimmedFile(sysfsDevicePath + QStringLiteral("/device"));
        const auto deviceNumber = deviceNumberForPath(nodePath);
        if (!deviceNumber.has_value()) {
            continue;
        }

        devices.push_back(GpuDeviceInfo{
            .nodePath = nodePath,
            .sysfsDevicePath = sysfsDevicePath,
            .vendorId = vendorId,
            .deviceId = deviceId,
            .vendor = gpuVendorFromId(vendorId),
            .discrete = isDiscretePciDevicePath(sysfsDevicePath),
            .deviceNumber = *deviceNumber,
        });
    }

    return devices;
}

QString describeGpuDevices(const QVector<GpuDeviceInfo> &devices) {
    QStringList parts;
    parts.reserve(devices.size());
    for (const auto &device : devices) {
        parts.push_back(QStringLiteral("%1:%2:%3")
                            .arg(device.nodePath,
                                 gpuVendorString(device.vendor),
                                 device.discrete ? QStringLiteral("discrete")
                                                 : QStringLiteral("integrated")));
    }
    return parts.isEmpty() ? QStringLiteral("<none>") : parts.join(QLatin1Char(','));
}

std::optional<GpuDeviceInfo> gpuInfoForPath(
    const QVector<GpuDeviceInfo> &devices,
    const QString &path
) {
    for (const auto &device : devices) {
        if (device.nodePath == path) {
            return device;
        }
    }
    return std::nullopt;
}

QString selectGpuDevicePath(
    const QVector<GpuDeviceInfo> &devices,
    const QString &policy,
    const QString &preferredPath
) {
    if (devices.isEmpty()) {
        return QString();
    }

    if (policy == QLatin1String("auto") && !preferredPath.isEmpty()) {
        return preferredPath;
    }

    QVector<GpuDeviceInfo> candidates;
    candidates.reserve(devices.size());
    for (const auto &device : devices) {
        if (gpuMatchesPolicy(device, policy)) {
            candidates.push_back(device);
        }
    }

    if (candidates.isEmpty()) {
        if (!preferredPath.isEmpty()) {
            return preferredPath;
        }
        candidates = devices;
    }

    int bestIndex = -1;
    int bestScore = std::numeric_limits<int>::min();
    for (int i = 0; i < candidates.size(); ++i) {
        const int score = scoreGpuForPolicy(candidates[i], policy, preferredPath);
        if (bestIndex < 0 || score > bestScore ||
            (score == bestScore && candidates[i].nodePath < candidates[bestIndex].nodePath)) {
            bestIndex = i;
            bestScore = score;
        }
    }

    return bestIndex >= 0 ? candidates[bestIndex].nodePath : QString();
}

QStringList reorderDecodeBackendsForGpu(QStringList backends, GpuVendor vendor) {
    for (QString &backend : backends) {
        backend = backend.trimmed().toLower();
    }
    backends.removeAll(QString());
    backends.removeDuplicates();

    auto preferFront = [&](const QString &backend) {
        backends.removeAll(backend);
        backends.prepend(backend);
    };

    if (vendor == GpuVendor::Nvidia) {
        preferFront(QStringLiteral("cuda"));
    } else if (vendor == GpuVendor::Amd || vendor == GpuVendor::Intel) {
        preferFront(QStringLiteral("vaapi"));
    }

    backends.removeAll(QStringLiteral("software"));
    backends.append(QStringLiteral("software"));
    return backends;
}

QString describeAvFrame(const AVFrame *frame) {
    if (frame == nullptr) {
        return QStringLiteral("frame=null");
    }

    QStringList parts;
    parts << QStringLiteral("fmt=%1").arg(avPixelFormatString(frame->format))
          << QStringLiteral("size=%1x%2").arg(frame->width).arg(frame->height);

    if (frame->hw_frames_ctx != nullptr) {
        const auto *hwFrames = reinterpret_cast<const AVHWFramesContext *>(frame->hw_frames_ctx->data);
        if (hwFrames != nullptr) {
            parts << QStringLiteral("hw_format=%1").arg(avPixelFormatString(hwFrames->format))
                  << QStringLiteral("sw_format=%1").arg(avPixelFormatString(hwFrames->sw_format));
        }
    }

    if (frame->format == AV_PIX_FMT_DRM_PRIME && frame->data[0] != nullptr) {
        const auto *drm = reinterpret_cast<const AVDRMFrameDescriptor *>(frame->data[0]);
        parts << QStringLiteral("drm_objects=%1").arg(drm->nb_objects)
              << QStringLiteral("drm_layers=%1").arg(drm->nb_layers);

        if (drm->nb_objects > 0) {
            const auto &object = drm->objects[0];
            parts << QStringLiteral("drm_modifier=%1").arg(dmabufModifierString(object.format_modifier));
        }
        if (drm->nb_layers > 0) {
            const auto &layer = drm->layers[0];
            parts << QStringLiteral("drm_layer0_format=%1").arg(drmFormatString(layer.format))
                  << QStringLiteral("drm_layer0_planes=%1").arg(layer.nb_planes);
        }
    }

    return parts.join(QLatin1Char(' '));
}

QString describeDerivedDrmFrame(const AVFrame *frame) {
    if (frame == nullptr || frame->hw_frames_ctx == nullptr) {
        return QStringLiteral("derive=unavailable");
    }

    AVFrame *derived = av_frame_alloc();
    if (derived == nullptr) {
        return QStringLiteral("derive=alloc_failed");
    }

    derived->width = frame->width;
    derived->height = frame->height;
    derived->format = AV_PIX_FMT_DRM_PRIME;
    derived->hw_frames_ctx = av_buffer_ref(frame->hw_frames_ctx);
    if (derived->hw_frames_ctx == nullptr) {
        av_frame_free(&derived);
        return QStringLiteral("derive=hw_frames_ref_failed");
    }

    const int rc = av_hwframe_map(derived, frame, AV_HWFRAME_MAP_READ | AV_HWFRAME_MAP_DIRECT);
    if (rc < 0) {
        av_frame_free(&derived);
        return QStringLiteral("derive=map_failed(%1)").arg(rc);
    }

    const QString description = describeAvFrame(derived);
    av_frame_free(&derived);
    return QStringLiteral("derive_ok(%1)").arg(description);
}

uint32_t modifierHi(uint64_t modifier) {
    return static_cast<uint32_t>(modifier >> 32);
}

uint32_t modifierLo(uint64_t modifier) {
    return static_cast<uint32_t>(modifier & 0xffffffffu);
}

std::optional<dev_t> parseDeviceNumber(const wl_array *array) {
    if (array == nullptr || array->data == nullptr || array->size != sizeof(dev_t)) {
        return std::nullopt;
    }

    dev_t device = 0;
    std::memcpy(&device, array->data, sizeof(dev_t));
    return device;
}

QString firstExistingDrmNode(const QStringList &patterns) {
    const QDir driDir(QStringLiteral("/dev/dri"));
    if (!driDir.exists()) {
        return QString();
    }

    for (const QString &name : driDir.entryList(patterns, QDir::System | QDir::Files, QDir::Name)) {
        const QString path = driDir.absoluteFilePath(name);
        if (QFileInfo::exists(path)) {
            return path;
        }
    }

    return QString();
}

QString drmNodePathForDevice(dev_t device) {
    const QDir driDir(QStringLiteral("/dev/dri"));
    if (!driDir.exists()) {
        return QString();
    }

    const QStringList names = driDir.entryList(
        QStringList{QStringLiteral("renderD*"), QStringLiteral("card*")},
        QDir::System | QDir::Files,
        QDir::Name
    );
    for (const QString &name : names) {
        const QString path = driDir.absoluteFilePath(name);
        struct stat st {};
        if (::stat(path.toUtf8().constData(), &st) != 0) {
            continue;
        }
        if (!S_ISCHR(st.st_mode)) {
            continue;
        }
        if (st.st_rdev == device) {
            return path;
        }
    }

    return QString();
}

QRectF cropRectFor(const QSize &sourceSize, const std::optional<CropRect> &crop) {
    const QRectF full(0.0, 0.0, sourceSize.width(), sourceSize.height());
    if (!crop.has_value()) {
        return full;
    }

    const CropRect value = *crop;
    const QRectF normalized(
        clamp01(value.x) * sourceSize.width(),
        clamp01(value.y) * sourceSize.height(),
        clamp01(value.width) * sourceSize.width(),
        clamp01(value.height) * sourceSize.height()
    );
    if (!normalized.isValid()) {
        return full;
    }

    return clampRectToBounds(normalized, full);
}

QRectF coverSourceRect(const QRectF &sourceRect, const QSize &targetSize) {
    if (!sourceRect.isValid() || targetSize.isEmpty()) {
        return sourceRect;
    }

    const double targetAspect = static_cast<double>(targetSize.width()) / targetSize.height();
    const double sourceAspect = sourceRect.width() / sourceRect.height();
    if (targetAspect <= 0.0 || sourceAspect <= 0.0) {
        return sourceRect;
    }

    QRectF result = sourceRect;
    if (targetAspect > sourceAspect) {
        const double desiredHeight = sourceRect.width() / targetAspect;
        result.setHeight(std::min(sourceRect.height(), desiredHeight));
        result.moveTop(sourceRect.center().y() - result.height() / 2.0);
    } else {
        const double desiredWidth = sourceRect.height() * targetAspect;
        result.setWidth(std::min(sourceRect.width(), desiredWidth));
        result.moveLeft(sourceRect.center().x() - result.width() / 2.0);
    }

    return clampRectToBounds(result, sourceRect);
}

void paintImageCover(
    QPainter &painter,
    const QImage &image,
    const QSize &targetSize,
    const std::optional<CropRect> &crop,
    qreal opacity
) {
    if (image.isNull() || targetSize.isEmpty() || opacity <= 0.0) {
        return;
    }

    const QRectF source = coverSourceRect(cropRectFor(image.size(), crop), targetSize);
    painter.save();
    painter.setOpacity(opacity);
    painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
    painter.drawImage(
        QRectF(0.0, 0.0, targetSize.width(), targetSize.height()),
        image,
        source
    );
    painter.restore();
}

std::optional<CropRect> parseCrop(const QJsonValue &value) {
    if (value.isNull() || value.isUndefined()) {
        return std::nullopt;
    }
    const QJsonObject obj = value.toObject();
    if (obj.isEmpty()) {
        return std::nullopt;
    }
    return CropRect{
        .x = obj.value(QStringLiteral("x")).toDouble(0.0),
        .y = obj.value(QStringLiteral("y")).toDouble(0.0),
        .width = obj.value(QStringLiteral("width")).toDouble(1.0),
        .height = obj.value(QStringLiteral("height")).toDouble(1.0),
    };
}

SnapshotModel parseSnapshot(const QJsonObject &payload) {
    SnapshotModel model;

    const QJsonObject transition = payload.value(QStringLiteral("transition")).toObject();
    model.transitionDurationMs =
        transition.value(QStringLiteral("duration_ms")).toInt(0);
    model.fallbackSource =
        payload.value(QStringLiteral("fallback_source")).toString();

    const QJsonObject renderer = payload.value(QStringLiteral("renderer")).toObject();
    const QJsonArray decodeBackendOrder =
        renderer.value(QStringLiteral("decode_backend_order")).toArray();
    model.presentBackend =
        normalizePresentBackend(renderer.value(QStringLiteral("present_backend")).toString());
    model.decodeDevicePolicy =
        normalizeGpuPolicy(
            renderer.value(QStringLiteral("decode_device_policy")).toString(),
            QStringLiteral("same-as-render")
        );
    model.renderDevicePolicy =
        normalizeGpuPolicy(
            renderer.value(QStringLiteral("render_device_policy")).toString(),
            QStringLiteral("same-as-compositor")
        );
    model.allowCrossGpu = renderer.value(QStringLiteral("allow_cross_gpu")).toBool(false);
    for (const QJsonValue &entry : decodeBackendOrder) {
        const QString backend = entry.toString().trimmed().toLower();
        if (!backend.isEmpty() && !model.decodeBackendOrder.contains(backend)) {
            model.decodeBackendOrder.push_back(backend);
        }
    }

    const QJsonObject sources = payload.value(QStringLiteral("sources")).toObject();
    for (auto it = sources.begin(); it != sources.end(); ++it) {
        const QJsonObject source = it.value().toObject();
        model.sources.insert(
            it.key(),
            SourceSpec{
                .id = source.value(QStringLiteral("id")).toString(it.key()),
                .path = source.value(QStringLiteral("path")).toString(),
                .name = source.value(QStringLiteral("name")).toString(it.key()),
                .kind = source.value(QStringLiteral("kind")).toString(),
                .loopEnabled = source.value(QStringLiteral("loop")).toBool(true),
                .mute = source.value(QStringLiteral("mute")).toBool(true),
            }
        );
    }

    const QJsonObject views = payload.value(QStringLiteral("views")).toObject();
    for (auto it = views.begin(); it != views.end(); ++it) {
        const QJsonObject view = it.value().toObject();
        model.views.insert(
            it.key(),
            ViewSpec{
                .output = view.value(QStringLiteral("output")).toString(it.key()),
                .source = view.value(QStringLiteral("source")).toString(),
                .fit = view.value(QStringLiteral("fit")).toString(QStringLiteral("cover")),
                .crop = parseCrop(view.value(QStringLiteral("crop"))),
            }
        );
    }

    return model;
}

} // namespace quicksov::wallpaper::renderer
