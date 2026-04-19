// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "../../wallpaper_ffmpeg/src/WallpaperVideo.hpp"

#include <algorithm>
#include <array>
#include <cerrno>
#include <cstring>
#include <map>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <fcntl.h>
#include <sys/stat.h>
#include <linux/memfd.h>
#include <sys/mman.h>
#include <unistd.h>

#include <QByteArray>
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QElapsedTimer>
#include <QFileInfo>
#include <QHash>
#include <QImage>
#include <QImageReader>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QLocalSocket>
#include <QPainter>
#include <QPointer>
#include <QSocketNotifier>
#include <QStringList>
#include <QTimer>
#include <QUrl>

extern "C" {
#include <gbm.h>
#include <drm_fourcc.h>
#include <wayland-client.h>
}

#define namespace namespace_
#include "wlr-layer-shell-unstable-v1-client-protocol.h"
#undef namespace
#include "linux-dmabuf-v1-client-protocol.h"

namespace {

constexpr const char *kLogPrefix = "[wallpaper-native]";
constexpr const char *kNamespace = "quicksov-wallpaper";
constexpr int kTransitionFrameMs = 16;
constexpr uint32_t kDmabufDrmFormat = DRM_FORMAT_ARGB8888;

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

double clamp01(double value) {
    return std::clamp(value, 0.0, 1.0);
}

QString normalizePresentBackend(QString backend) {
    backend = backend.trimmed().toLower();
    if (backend == QLatin1String("shm") || backend == QLatin1String("dmabuf")) {
        return backend;
    }
    return QStringLiteral("auto");
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
    QString presentBackend = QStringLiteral("auto");
    int transitionDurationMs = 0;
};

struct DmabufFormatModifier {
    uint32_t format = 0;
    uint64_t modifier = DRM_FORMAT_MOD_INVALID;

    auto operator==(const DmabufFormatModifier &) const -> bool = default;
};

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

class SourceSession final : public QObject {
    Q_OBJECT

public:
    struct StatsSnapshot {
        QString id;
        QString kind;
        QString status;
        QString hwdecCurrent;
        QSize videoSize;
        QSize frameSize;
        quint64 decodedFrames = 0;
        bool ready = false;
    };

    explicit SourceSession(const SourceSpec &spec, const QStringList &decodeBackendOrder, QObject *parent = nullptr)
        : QObject(parent)
        , m_spec(spec)
        , m_decodeBackendOrder(decodeBackendOrder) {
        if (m_spec.kind == QStringLiteral("video")) {
            auto *video = new WallpaperVideo(this);
            video->setDebugName(QStringLiteral("source:%1").arg(m_spec.id));
            video->setMuted(m_spec.mute);
            video->setLoopEnabled(m_spec.loopEnabled);
            video->setPreferredHwdecOrder(m_decodeBackendOrder);
            connect(video, &WallpaperVideo::frameAvailable, this, &SourceSession::updated);
            connect(video, &WallpaperVideo::readyChanged, this, &SourceSession::updated);
            connect(video, &WallpaperVideo::statusChanged, this, &SourceSession::updated);
            connect(video, &WallpaperVideo::errorStringChanged, this, &SourceSession::updated);
            connect(video, &WallpaperVideo::hwdecCurrentChanged, this, &SourceSession::updated);
            video->setSource(QUrl::fromLocalFile(m_spec.path));
            m_video = video;
        } else {
            QImageReader reader(m_spec.path);
            reader.setAutoTransform(true);
            m_image = reader.read();
            if (m_image.isNull()) {
                qWarning().noquote() << kLogPrefix << "failed to load image wallpaper"
                                     << m_spec.id << m_spec.path << reader.errorString();
            }
        }
    }

    [[nodiscard]] const SourceSpec &spec() const {
        return m_spec;
    }

    [[nodiscard]] bool isVideo() const {
        return m_video != nullptr;
    }

    [[nodiscard]] bool ready() const {
        if (m_video != nullptr) {
            return m_video->isReady() && m_video->frameSnapshot().hasFrame;
        }
        return !m_image.isNull();
    }

    [[nodiscard]] bool matches(const SourceSpec &spec, const QStringList &decodeBackendOrder) const {
        return m_spec.path == spec.path &&
               m_spec.kind == spec.kind &&
               m_spec.loopEnabled == spec.loopEnabled &&
               m_spec.mute == spec.mute &&
               m_decodeBackendOrder == decodeBackendOrder;
    }

    [[nodiscard]] StatsSnapshot statsSnapshot() const {
        StatsSnapshot stats{
            .id = m_spec.id,
            .kind = m_spec.kind,
            .status = m_image.isNull() ? QStringLiteral("empty") : QStringLiteral("ready"),
            .ready = !m_image.isNull(),
        };

        if (m_video != nullptr) {
            const auto videoStats = m_video->statsSnapshot();
            stats.status = videoStats.status;
            stats.hwdecCurrent = videoStats.hwdecCurrent;
            stats.videoSize = videoStats.videoSize;
            stats.frameSize = videoStats.frameSize;
            stats.decodedFrames = videoStats.decodedFrames;
            stats.ready = ready();
        }

        return stats;
    }

    void updateRenderHint(QObject *owner, const QSize &size) {
        if (m_video != nullptr) {
            m_video->updateRenderTargetHint(owner, size);
        }
    }

    void removeRenderHint(QObject *owner) {
        if (m_video != nullptr) {
            m_video->removeRenderTargetHint(owner);
        }
    }

    bool paint(QPainter &painter, const QSize &targetSize, const std::optional<CropRect> &crop, qreal opacity) const {
        if (m_video != nullptr) {
            const auto frame = m_video->frameSnapshot();
            if (!frame.hasFrame || frame.image.isNull()) {
                return false;
            }
            paintImageCover(painter, frame.image, targetSize, crop, opacity);
            return true;
        }

        if (m_image.isNull()) {
            return false;
        }

        paintImageCover(painter, m_image, targetSize, crop, opacity);
        return true;
    }

signals:
    void updated();

private:
    SourceSpec m_spec;
    QStringList m_decodeBackendOrder;
    QImage m_image;
    QPointer<WallpaperVideo> m_video;
};

class WaylandRenderer;

class OutputSurface final : public QObject {
    Q_OBJECT

public:
    struct StatsSnapshot {
        QString outputName;
        QString sourceId;
        QString requestedPresentBackend;
        QString resolvedPresentBackend;
        QString presentBackendFallbackReason;
        QSize logicalSize;
        QSize pixelSize;
        quint64 committedFrames = 0;
        quint64 presentedFrames = 0;
        quint64 bufferStarvedFrames = 0;
        bool configured = false;
    };

    OutputSurface(WaylandRenderer *renderer, uint32_t registryName, wl_output *output);
    ~OutputSurface() override;

    [[nodiscard]] uint32_t registryName() const;
    [[nodiscard]] QString outputName() const;
    [[nodiscard]] SourceSession *boundSource() const;
    [[nodiscard]] StatsSnapshot statsSnapshot() const;

    void setScale(int scale);
    void setName(const QString &name);
    void createSurface();
    void destroySurface();
    void applySnapshot(const SnapshotModel &snapshot, const std::map<QString, std::shared_ptr<SourceSession>> &sources);
    void scheduleRender();
    void onFrameCallbackDone();
    void onBufferReleased();
    void handleClosed();
    void handleConfigure(uint32_t serial, uint32_t width, uint32_t height);
    void updateVideoHint();

    wl_output *output() const;

private:
    struct ShmBuffer {
        OutputSurface *owner = nullptr;
        wl_buffer *buffer = nullptr;
        void *data = nullptr;
        size_t bytes = 0;
        int width = 0;
        int height = 0;
        int stride = 0;
        bool busy = false;
    };

    struct DmaBuffer {
        OutputSurface *owner = nullptr;
        wl_buffer *buffer = nullptr;
        zwp_linux_buffer_params_v1 *params = nullptr;
        gbm_bo *bo = nullptr;
        void *data = nullptr;
        void *mapData = nullptr;
        int width = 0;
        int height = 0;
        int stride = 0;
        uint32_t format = kDmabufDrmFormat;
        uint64_t modifier = DRM_FORMAT_MOD_INVALID;
        bool busy = false;
        bool pending = false;
    };

    void setBinding(std::shared_ptr<SourceSession> source, std::optional<CropRect> crop, int transitionMs);
    void detachCurrentSourceHint();
    void ensureShmBuffers();
    bool ensureDmabufBuffers();
    void destroyBuffers();
    void destroyShmBuffers();
    void destroyDmabufBuffers();
    ShmBuffer *nextFreeShmBuffer();
    DmaBuffer *nextFreeDmabufBuffer();
    bool createDmabufBuffer(DmaBuffer &buffer);
    void releaseDmabufBuffer(DmaBuffer &buffer);
    void disableDmabuf(const QString &reason);
    bool supportsDmabufModifier(uint32_t format, uint64_t modifier) const;
    std::vector<uint64_t> supportedDmabufModifiers(uint32_t format) const;
    void render();
    void startTransition(int durationMs);
    void stopTransition();
    void capturePreviousImage();
    void flush();
    void onDmabufBufferCreated();
    void onDmabufBufferFailed(DmaBuffer *buffer);

    static void layerSurfaceConfigure(
        void *data,
        zwlr_layer_surface_v1 *layerSurface,
        uint32_t serial,
        uint32_t width,
        uint32_t height
    );
    static void layerSurfaceClosed(void *data, zwlr_layer_surface_v1 *layerSurface);
    static void frameDone(void *data, wl_callback *callback, uint32_t time);
    static void shmBufferReleased(void *data, wl_buffer *buffer);
    static void dmabufBufferReleased(void *data, wl_buffer *buffer);
    static void dmabufParamsCreated(
        void *data,
        zwp_linux_buffer_params_v1 *params,
        wl_buffer *buffer
    );
    static void dmabufParamsFailed(void *data, zwp_linux_buffer_params_v1 *params);
    static void dmabufFeedbackDone(void *data, zwp_linux_dmabuf_feedback_v1 *feedback);
    static void dmabufFeedbackFormatTable(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        int32_t fd,
        uint32_t size
    );
    static void dmabufFeedbackMainDevice(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        wl_array *device
    );
    static void dmabufFeedbackTrancheDone(void *data, zwp_linux_dmabuf_feedback_v1 *feedback);
    static void dmabufFeedbackTrancheTargetDevice(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        wl_array *device
    );
    static void dmabufFeedbackTrancheFormats(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        wl_array *indices
    );
    static void dmabufFeedbackTrancheFlags(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        uint32_t flags
    );

    WaylandRenderer *m_renderer = nullptr;
    uint32_t m_registryName = 0;
    wl_output *m_output = nullptr;
    wl_surface *m_surface = nullptr;
    zwlr_layer_surface_v1 *m_layerSurface = nullptr;
    zwp_linux_dmabuf_feedback_v1 *m_surfaceDmabufFeedback = nullptr;
    wl_callback *m_frameCallback = nullptr;
    QString m_outputName;
    QString m_requestedPresentBackend = QStringLiteral("auto");
    QString m_resolvedPresentBackend = QStringLiteral("shm");
    QString m_presentBackendFallbackReason;
    int m_scale = 1;
    QSize m_logicalSize;
    QSize m_pixelSize;
    bool m_configured = false;
    bool m_dirty = false;
    std::shared_ptr<SourceSession> m_source;
    std::optional<CropRect> m_crop;
    std::array<ShmBuffer, 2> m_shmBuffers;
    std::array<DmaBuffer, 2> m_dmabufBuffers;
    int m_lastPresentedIndex = -1;
    QString m_lastPresentedBackend = QStringLiteral("shm");
    QImage m_previousImage;
    QElapsedTimer m_transitionClock;
    QTimer m_transitionTimer;
    int m_transitionDurationMs = 0;
    quint64 m_committedFrames = 0;
    quint64 m_presentedFrames = 0;
    quint64 m_bufferStarvedFrames = 0;
    QByteArray m_dmabufFormatTable;
    std::vector<DmabufFormatModifier> m_surfaceDmabufFormats;
    bool m_dmabufFeedbackReady = false;
    bool m_dmabufDisabled = false;
};

class WaylandRenderer final : public QObject {
    Q_OBJECT

public:
    struct PresentBackendSelection {
        QString requested;
        QString resolved;
        QString fallbackReason;
    };

    explicit WaylandRenderer(QObject *parent = nullptr)
        : QObject(parent) {}

    ~WaylandRenderer() override {
        if (m_displayNotifier != nullptr) {
            delete m_displayNotifier;
            m_displayNotifier = nullptr;
        }

        m_outputs.clear();

        if (m_defaultDmabufFeedback != nullptr) {
            zwp_linux_dmabuf_feedback_v1_destroy(m_defaultDmabufFeedback);
            m_defaultDmabufFeedback = nullptr;
        }
        if (m_layerShell != nullptr) {
            zwlr_layer_shell_v1_destroy(m_layerShell);
            m_layerShell = nullptr;
        }
        if (m_linuxDmabuf != nullptr) {
            zwp_linux_dmabuf_v1_destroy(m_linuxDmabuf);
            m_linuxDmabuf = nullptr;
        }
        if (m_gbmDevice != nullptr) {
            gbm_device_destroy(m_gbmDevice);
            m_gbmDevice = nullptr;
        }
        if (m_gbmDeviceFd >= 0) {
            ::close(m_gbmDeviceFd);
            m_gbmDeviceFd = -1;
        }
        if (m_shm != nullptr) {
            wl_shm_destroy(m_shm);
            m_shm = nullptr;
        }
        if (m_compositor != nullptr) {
            wl_compositor_destroy(m_compositor);
            m_compositor = nullptr;
        }
        if (m_registry != nullptr) {
            wl_registry_destroy(m_registry);
            m_registry = nullptr;
        }
        if (m_display != nullptr) {
            wl_display_disconnect(m_display);
            m_display = nullptr;
        }
    }

    bool initialize(QString *error) {
        m_display = wl_display_connect(nullptr);
        if (m_display == nullptr) {
            *error = QStringLiteral("wl_display_connect failed");
            return false;
        }

        m_registry = wl_display_get_registry(m_display);
        if (m_registry == nullptr) {
            *error = QStringLiteral("wl_display_get_registry failed");
            return false;
        }

        static constexpr wl_registry_listener registryListener = {
            .global = &WaylandRenderer::registryGlobal,
            .global_remove = &WaylandRenderer::registryGlobalRemove,
        };
        wl_registry_add_listener(m_registry, &registryListener, this);

        if (wl_display_roundtrip(m_display) < 0 || wl_display_roundtrip(m_display) < 0) {
            *error = QStringLiteral("wl_display_roundtrip failed");
            return false;
        }

        if (m_compositor == nullptr) {
            *error = QStringLiteral("wl_compositor missing");
            return false;
        }
        if (m_shm == nullptr) {
            *error = QStringLiteral("wl_shm missing");
            return false;
        }
        if (m_layerShell == nullptr) {
            *error = QStringLiteral("zwlr_layer_shell_v1 missing");
            return false;
        }

        for (auto &entry : m_outputs) {
            entry.second->createSurface();
        }

        const int fd = wl_display_get_fd(m_display);
        if (fd < 0) {
            *error = QStringLiteral("wl_display_get_fd failed");
            return false;
        }

        m_displayNotifier = new QSocketNotifier(fd, QSocketNotifier::Read, this);
        connect(m_displayNotifier, &QSocketNotifier::activated, this, [this]() {
            if (wl_display_dispatch(m_display) < 0) {
                emit fatalError(QStringLiteral("wl_display_dispatch failed"));
                return;
            }
            wl_display_flush(m_display);
        });

        m_telemetryTimer.setInterval(5000);
        connect(&m_telemetryTimer, &QTimer::timeout, this, &WaylandRenderer::logTelemetry);
        m_telemetryTimer.start();

        qInfo().noquote() << kLogPrefix << "wayland renderer initialized"
                          << "outputs =" << static_cast<int>(m_outputs.size())
                          << "dmabuf_advertised =" << m_dmabufAdvertised
                          << "dmabuf_version =" << m_dmabufVersion
                          << "dmabuf_formats =" << m_dmabufFormatCount
                          << "dmabuf_modifiers =" << m_dmabufModifierCount;
        return true;
    }

    void applySnapshot(const SnapshotModel &snapshot) {
        m_snapshot = snapshot;

        for (const auto &[id, spec] : snapshot.sources.asKeyValueRange()) {
            auto it = m_sources.find(id);
            if (it != m_sources.end()) {
                if (it->second->matches(spec, snapshot.decodeBackendOrder)) {
                    continue;
                }
            }

            auto session = std::make_shared<SourceSession>(spec, snapshot.decodeBackendOrder);
            connect(session.get(), &SourceSession::updated, this, [this, raw = session.get()]() {
                onSourceUpdated(raw);
            });
            m_sources[id] = std::move(session);
        }

        for (auto &entry : m_outputs) {
            entry.second->applySnapshot(m_snapshot, m_sources);
        }

        for (auto it = m_sources.begin(); it != m_sources.end();) {
            if (!m_snapshot.sources.contains(it->first)) {
                it = m_sources.erase(it);
            } else {
                ++it;
            }
        }

        flush();
    }

    wl_compositor *compositor() const {
        return m_compositor;
    }

    wl_shm *shm() const {
        return m_shm;
    }

    zwp_linux_dmabuf_v1 *linuxDmabuf() const {
        return m_linuxDmabuf;
    }

    [[nodiscard]] bool ensureGbmDevice(QString *reason) {
        if (!m_dmabufAdvertised || m_linuxDmabuf == nullptr) {
            if (reason != nullptr) {
                *reason = QStringLiteral("dmabuf_not_advertised");
            }
            return false;
        }

        if (m_gbmDevice != nullptr) {
            if (reason != nullptr) {
                reason->clear();
            }
            return true;
        }

        if (m_gbmDeviceAttempted) {
            if (reason != nullptr) {
                *reason = m_gbmDeviceFailureReason;
            }
            return false;
        }

        m_gbmDeviceAttempted = true;

        QString path;
        if (m_dmabufMainDevice.has_value()) {
            path = drmNodePathForDevice(*m_dmabufMainDevice);
        }
        if (path.isEmpty()) {
            path = firstExistingDrmNode(QStringList{QStringLiteral("renderD*"), QStringLiteral("card*")});
        }
        if (path.isEmpty()) {
            m_gbmDeviceFailureReason = QStringLiteral("drm_node_missing");
            if (reason != nullptr) {
                *reason = m_gbmDeviceFailureReason;
            }
            return false;
        }

        const int fd = ::open(path.toUtf8().constData(), O_RDWR | O_CLOEXEC);
        if (fd < 0) {
            m_gbmDeviceFailureReason = QStringLiteral("drm_node_open_failed");
            qWarning().noquote() << kLogPrefix << "failed to open DRM node"
                                 << path << std::strerror(errno);
            if (reason != nullptr) {
                *reason = m_gbmDeviceFailureReason;
            }
            return false;
        }

        gbm_device *device = gbm_create_device(fd);
        if (device == nullptr) {
            ::close(fd);
            m_gbmDeviceFailureReason = QStringLiteral("gbm_device_create_failed");
            if (reason != nullptr) {
                *reason = m_gbmDeviceFailureReason;
            }
            return false;
        }

        m_gbmDevice = device;
        m_gbmDeviceFd = fd;
        m_gbmDevicePath = path;
        m_gbmDeviceFailureReason.clear();
        qInfo().noquote() << kLogPrefix << "gbm device initialized"
                          << "path =" << m_gbmDevicePath
                          << "backend =" << QString::fromUtf8(gbm_device_get_backend_name(m_gbmDevice));

        if (reason != nullptr) {
            reason->clear();
        }
        return true;
    }

    gbm_device *gbmDevice() const {
        return m_gbmDevice;
    }

    [[nodiscard]] PresentBackendSelection resolvePresentBackend(const QString &requested) const {
        const QString normalizedRequested = normalizePresentBackend(requested);
        if (normalizedRequested == QLatin1String("shm")) {
            return PresentBackendSelection{
                .requested = normalizedRequested,
                .resolved = QStringLiteral("shm"),
            };
        }

        if (normalizedRequested == QLatin1String("dmabuf")) {
            if (!m_dmabufAdvertised) {
                return PresentBackendSelection{
                    .requested = normalizedRequested,
                    .resolved = QStringLiteral("shm"),
                    .fallbackReason = QStringLiteral("dmabuf_not_advertised"),
                };
            }
            return PresentBackendSelection{
                .requested = normalizedRequested,
                .resolved = QStringLiteral("dmabuf"),
            };
        }

        if (m_dmabufAdvertised) {
            return PresentBackendSelection{
                .requested = normalizedRequested,
                .resolved = QStringLiteral("dmabuf"),
            };
        }

        return PresentBackendSelection{
            .requested = normalizedRequested,
            .resolved = QStringLiteral("shm"),
            .fallbackReason = QStringLiteral("dmabuf_not_advertised"),
        };
    }

    [[nodiscard]] bool dmabufAdvertised() const {
        return m_dmabufAdvertised;
    }

    [[nodiscard]] uint32_t dmabufVersion() const {
        return m_dmabufVersion;
    }

    [[nodiscard]] quint64 dmabufFormatCount() const {
        return m_dmabufFormatCount;
    }

    [[nodiscard]] quint64 dmabufModifierCount() const {
        return m_dmabufModifierCount;
    }

    zwlr_layer_shell_v1 *layerShell() const {
        return m_layerShell;
    }

    void flush() {
        if (m_display != nullptr) {
            wl_display_flush(m_display);
        }
    }

    void rebindOutputs() {
        for (auto &entry : m_outputs) {
            entry.second->applySnapshot(m_snapshot, m_sources);
        }
    }

signals:
    void fatalError(const QString &message);

private:
    void logTelemetry() {
        qInfo().noquote() << kLogPrefix
                          << "telemetry"
                          << "sources =" << static_cast<int>(m_sources.size())
                          << "outputs =" << static_cast<int>(m_outputs.size())
                          << "requested_present_backend =" << m_snapshot.presentBackend
                          << "dmabuf_advertised =" << m_dmabufAdvertised
                          << "dmabuf_version =" << m_dmabufVersion
                          << "dmabuf_formats =" << m_dmabufFormatCount
                          << "dmabuf_modifiers =" << m_dmabufModifierCount;

        constexpr double intervalSeconds = 5.0;

        for (const auto &[id, source] : m_sources) {
            const auto stats = source->statsSnapshot();
            const quint64 previous = m_prevDecodedFrames.value(id, 0);
            const quint64 delta = stats.decodedFrames - previous;
            m_prevDecodedFrames.insert(id, stats.decodedFrames);

            qInfo().noquote() << kLogPrefix
                              << "source"
                              << id
                              << "kind =" << stats.kind
                              << "ready =" << stats.ready
                              << "status =" << stats.status
                              << "hwdec =" << (stats.hwdecCurrent.isEmpty() ? QStringLiteral("n/a") : stats.hwdecCurrent)
                              << "decoded_total =" << stats.decodedFrames
                              << "decoded_fps =" << QString::number(static_cast<double>(delta) / intervalSeconds, 'f', 1)
                              << "video =" << QStringLiteral("%1x%2").arg(stats.videoSize.width()).arg(stats.videoSize.height())
                              << "frame =" << QStringLiteral("%1x%2").arg(stats.frameSize.width()).arg(stats.frameSize.height());
        }

        for (const auto &[registryName, output] : m_outputs) {
            const auto stats = output->statsSnapshot();
            const QString key = stats.outputName.isEmpty()
                ? QString::number(registryName)
                : stats.outputName;
            const quint64 previousCommitted = m_prevCommittedFrames.value(key, 0);
            const quint64 previousPresented = m_prevPresentedFrames.value(key, 0);
            const quint64 previousStarved = m_prevBufferStarvedFrames.value(key, 0);
            const quint64 committedDelta = stats.committedFrames - previousCommitted;
            const quint64 presentedDelta = stats.presentedFrames - previousPresented;
            const quint64 starvedDelta = stats.bufferStarvedFrames - previousStarved;

            m_prevCommittedFrames.insert(key, stats.committedFrames);
            m_prevPresentedFrames.insert(key, stats.presentedFrames);
            m_prevBufferStarvedFrames.insert(key, stats.bufferStarvedFrames);

            qInfo().noquote() << kLogPrefix
                              << "output"
                              << (stats.outputName.isEmpty() ? QStringLiteral("<unnamed>") : stats.outputName)
                              << "source =" << (stats.sourceId.isEmpty() ? QStringLiteral("<none>") : stats.sourceId)
                              << "present_backend_requested =" << stats.requestedPresentBackend
                              << "present_backend_resolved =" << stats.resolvedPresentBackend
                              << "present_backend_fallback =" << (stats.presentBackendFallbackReason.isEmpty() ? QStringLiteral("none") : stats.presentBackendFallbackReason)
                              << "configured =" << stats.configured
                              << "logical =" << QStringLiteral("%1x%2").arg(stats.logicalSize.width()).arg(stats.logicalSize.height())
                              << "pixel =" << QStringLiteral("%1x%2").arg(stats.pixelSize.width()).arg(stats.pixelSize.height())
                              << "commit_total =" << stats.committedFrames
                              << "commit_fps =" << QString::number(static_cast<double>(committedDelta) / intervalSeconds, 'f', 1)
                              << "present_total =" << stats.presentedFrames
                              << "present_fps =" << QString::number(static_cast<double>(presentedDelta) / intervalSeconds, 'f', 1)
                              << "buffer_starved_total =" << stats.bufferStarvedFrames
                              << "buffer_starved_5s =" << starvedDelta;
        }
    }

    static void registryGlobal(
        void *data,
        wl_registry *registry,
        uint32_t name,
        const char *interface,
        uint32_t version
    ) {
        auto *self = static_cast<WaylandRenderer *>(data);
        const QByteArray iface(interface);

        if (iface == "wl_compositor") {
            self->m_compositor = static_cast<wl_compositor *>(
                wl_registry_bind(registry, name, &wl_compositor_interface, std::min(version, 4U))
            );
            return;
        }

        if (iface == "wl_shm") {
            self->m_shm = static_cast<wl_shm *>(
                wl_registry_bind(registry, name, &wl_shm_interface, 1)
            );
            return;
        }

        if (iface == "zwp_linux_dmabuf_v1") {
            self->m_linuxDmabuf = static_cast<zwp_linux_dmabuf_v1 *>(
                wl_registry_bind(registry, name, &zwp_linux_dmabuf_v1_interface, std::min(version, 4U))
            );
            self->m_dmabufAdvertised = true;
            self->m_dmabufVersion = std::min(version, 4U);
            static constexpr zwp_linux_dmabuf_v1_listener dmabufListener = {
                .format = &WaylandRenderer::dmabufFormat,
                .modifier = &WaylandRenderer::dmabufModifier,
            };
            zwp_linux_dmabuf_v1_add_listener(self->m_linuxDmabuf, &dmabufListener, self);
            if (self->m_dmabufVersion >= 4) {
                self->m_defaultDmabufFeedback =
                    zwp_linux_dmabuf_v1_get_default_feedback(self->m_linuxDmabuf);
                static constexpr zwp_linux_dmabuf_feedback_v1_listener feedbackListener = {
                    .done = &WaylandRenderer::defaultFeedbackDone,
                    .format_table = &WaylandRenderer::defaultFeedbackFormatTable,
                    .main_device = &WaylandRenderer::defaultFeedbackMainDevice,
                    .tranche_done = &WaylandRenderer::defaultFeedbackTrancheDone,
                    .tranche_target_device = &WaylandRenderer::defaultFeedbackTrancheTargetDevice,
                    .tranche_formats = &WaylandRenderer::defaultFeedbackTrancheFormats,
                    .tranche_flags = &WaylandRenderer::defaultFeedbackTrancheFlags,
                };
                zwp_linux_dmabuf_feedback_v1_add_listener(
                    self->m_defaultDmabufFeedback,
                    &feedbackListener,
                    self
                );
            }
            return;
        }

        if (iface == "zwlr_layer_shell_v1") {
            self->m_layerShell = static_cast<zwlr_layer_shell_v1 *>(
                wl_registry_bind(registry, name, &zwlr_layer_shell_v1_interface, std::min(version, 5U))
            );
            return;
        }

        if (iface == "wl_output") {
            auto *output = static_cast<wl_output *>(
                wl_registry_bind(registry, name, &wl_output_interface, std::min(version, 4U))
            );
            auto entry = std::make_unique<OutputSurface>(self, name, output);
            static constexpr wl_output_listener outputListener = {
                .geometry = [](void *, wl_output *, int32_t, int32_t, int32_t, int32_t, int32_t, const char *, const char *, int32_t) {},
                .mode = [](void *, wl_output *, uint32_t, int32_t, int32_t, int32_t) {},
                .done = [](void *, wl_output *) {},
                .scale = [](void *outputData, wl_output *, int32_t factor) {
                    static_cast<OutputSurface *>(outputData)->setScale(std::max(1, factor));
                },
                .name = [](void *outputData, wl_output *, const char *nameValue) {
                    static_cast<OutputSurface *>(outputData)->setName(QString::fromUtf8(nameValue));
                },
                .description = [](void *, wl_output *, const char *) {},
            };
            wl_output_add_listener(output, &outputListener, entry.get());
            self->m_outputs.emplace(name, std::move(entry));
            return;
        }
    }

    static void registryGlobalRemove(void *data, wl_registry *, uint32_t name) {
        auto *self = static_cast<WaylandRenderer *>(data);
        self->m_outputs.erase(name);
    }

    static void dmabufFormat(void *data, zwp_linux_dmabuf_v1 *, uint32_t) {
        auto *self = static_cast<WaylandRenderer *>(data);
        self->m_dmabufFormatCount += 1;
    }

    static void dmabufModifier(
        void *data,
        zwp_linux_dmabuf_v1 *,
        uint32_t,
        uint32_t,
        uint32_t
    ) {
        auto *self = static_cast<WaylandRenderer *>(data);
        self->m_dmabufModifierCount += 1;
    }

    static void defaultFeedbackDone(void *, zwp_linux_dmabuf_feedback_v1 *) {}

    static void defaultFeedbackFormatTable(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        int32_t fd,
        uint32_t
    ) {
        if (fd >= 0) {
            ::close(fd);
        }
    }

    static void defaultFeedbackMainDevice(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *,
        wl_array *device
    ) {
        auto *self = static_cast<WaylandRenderer *>(data);
        self->m_dmabufMainDevice = parseDeviceNumber(device);
    }

    static void defaultFeedbackTrancheDone(void *, zwp_linux_dmabuf_feedback_v1 *) {}

    static void defaultFeedbackTrancheTargetDevice(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        wl_array *
    ) {}

    static void defaultFeedbackTrancheFormats(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        wl_array *
    ) {}

    static void defaultFeedbackTrancheFlags(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        uint32_t
    ) {}

    void onSourceUpdated(SourceSession *source) {
        for (auto &entry : m_outputs) {
            if (entry.second->boundSource() == source) {
                entry.second->scheduleRender();
            }
        }
    }

    wl_display *m_display = nullptr;
    wl_registry *m_registry = nullptr;
    wl_compositor *m_compositor = nullptr;
    wl_shm *m_shm = nullptr;
    zwp_linux_dmabuf_v1 *m_linuxDmabuf = nullptr;
    zwp_linux_dmabuf_feedback_v1 *m_defaultDmabufFeedback = nullptr;
    zwlr_layer_shell_v1 *m_layerShell = nullptr;
    QSocketNotifier *m_displayNotifier = nullptr;
    QTimer m_telemetryTimer;
    SnapshotModel m_snapshot;
    std::map<uint32_t, std::unique_ptr<OutputSurface>> m_outputs;
    std::map<QString, std::shared_ptr<SourceSession>> m_sources;
    QHash<QString, quint64> m_prevDecodedFrames;
    QHash<QString, quint64> m_prevCommittedFrames;
    QHash<QString, quint64> m_prevPresentedFrames;
    QHash<QString, quint64> m_prevBufferStarvedFrames;
    bool m_dmabufAdvertised = false;
    uint32_t m_dmabufVersion = 0;
    quint64 m_dmabufFormatCount = 0;
    quint64 m_dmabufModifierCount = 0;
    std::optional<dev_t> m_dmabufMainDevice;
    gbm_device *m_gbmDevice = nullptr;
    int m_gbmDeviceFd = -1;
    bool m_gbmDeviceAttempted = false;
    QString m_gbmDevicePath;
    QString m_gbmDeviceFailureReason;
};

OutputSurface::OutputSurface(WaylandRenderer *renderer, uint32_t registryName, wl_output *output)
    : QObject(renderer)
    , m_renderer(renderer)
    , m_registryName(registryName)
    , m_output(output) {
    m_transitionTimer.setInterval(kTransitionFrameMs);
    connect(&m_transitionTimer, &QTimer::timeout, this, [this]() {
        if (m_transitionDurationMs <= 0 ||
            !m_transitionClock.isValid() ||
            m_transitionClock.elapsed() >= m_transitionDurationMs) {
            stopTransition();
        }
        scheduleRender();
    });
}

OutputSurface::~OutputSurface() {
    detachCurrentSourceHint();
    destroySurface();
    if (m_output != nullptr) {
        wl_output_destroy(m_output);
        m_output = nullptr;
    }
}

uint32_t OutputSurface::registryName() const {
    return m_registryName;
}

QString OutputSurface::outputName() const {
    return m_outputName;
}

SourceSession *OutputSurface::boundSource() const {
    return m_source.get();
}

OutputSurface::StatsSnapshot OutputSurface::statsSnapshot() const {
    return StatsSnapshot{
        .outputName = m_outputName,
        .sourceId = (m_source != nullptr) ? m_source->spec().id : QString(),
        .requestedPresentBackend = m_requestedPresentBackend,
        .resolvedPresentBackend = m_resolvedPresentBackend,
        .presentBackendFallbackReason = m_presentBackendFallbackReason,
        .logicalSize = m_logicalSize,
        .pixelSize = m_pixelSize,
        .committedFrames = m_committedFrames,
        .presentedFrames = m_presentedFrames,
        .bufferStarvedFrames = m_bufferStarvedFrames,
        .configured = m_configured,
    };
}

void OutputSurface::setScale(int scale) {
    if (m_scale == scale) {
        return;
    }
    m_scale = std::max(1, scale);
    if (m_logicalSize.isValid()) {
        m_pixelSize = QSize(m_logicalSize.width() * m_scale, m_logicalSize.height() * m_scale);
    }
    destroyBuffers();
    updateVideoHint();
    scheduleRender();
}

void OutputSurface::setName(const QString &name) {
    if (m_outputName == name) {
        return;
    }
    m_outputName = name;
    qInfo().noquote() << kLogPrefix << "output discovered" << m_outputName;
    m_renderer->rebindOutputs();
}

void OutputSurface::createSurface() {
    if (m_surface != nullptr || m_renderer->compositor() == nullptr || m_renderer->layerShell() == nullptr) {
        return;
    }

    m_surface = wl_compositor_create_surface(m_renderer->compositor());
    if (m_surface == nullptr) {
        qWarning().noquote() << kLogPrefix << "failed to create wl_surface";
        return;
    }

    m_layerSurface = zwlr_layer_shell_v1_get_layer_surface(
        m_renderer->layerShell(),
        m_surface,
        m_output,
        ZWLR_LAYER_SHELL_V1_LAYER_BACKGROUND,
        kNamespace
    );
    if (m_layerSurface == nullptr) {
        qWarning().noquote() << kLogPrefix << "failed to create layer surface";
        wl_surface_destroy(m_surface);
        m_surface = nullptr;
        return;
    }

    static constexpr zwlr_layer_surface_v1_listener layerListener = {
        .configure = &OutputSurface::layerSurfaceConfigure,
        .closed = &OutputSurface::layerSurfaceClosed,
    };
    zwlr_layer_surface_v1_add_listener(m_layerSurface, &layerListener, this);
    zwlr_layer_surface_v1_set_anchor(
        m_layerSurface,
        ZWLR_LAYER_SURFACE_V1_ANCHOR_TOP |
            ZWLR_LAYER_SURFACE_V1_ANCHOR_BOTTOM |
            ZWLR_LAYER_SURFACE_V1_ANCHOR_LEFT |
            ZWLR_LAYER_SURFACE_V1_ANCHOR_RIGHT
    );
    zwlr_layer_surface_v1_set_size(m_layerSurface, 0, 0);
    zwlr_layer_surface_v1_set_keyboard_interactivity(
        m_layerSurface,
        ZWLR_LAYER_SURFACE_V1_KEYBOARD_INTERACTIVITY_NONE
    );
    zwlr_layer_surface_v1_set_exclusive_zone(m_layerSurface, -1);

    wl_region *region = wl_compositor_create_region(m_renderer->compositor());
    wl_surface_set_input_region(m_surface, region);
    wl_region_destroy(region);

    if (m_renderer->linuxDmabuf() != nullptr && m_renderer->dmabufVersion() >= 4) {
        m_surfaceDmabufFeedback =
            zwp_linux_dmabuf_v1_get_surface_feedback(m_renderer->linuxDmabuf(), m_surface);
        static constexpr zwp_linux_dmabuf_feedback_v1_listener feedbackListener = {
            .done = &OutputSurface::dmabufFeedbackDone,
            .format_table = &OutputSurface::dmabufFeedbackFormatTable,
            .main_device = &OutputSurface::dmabufFeedbackMainDevice,
            .tranche_done = &OutputSurface::dmabufFeedbackTrancheDone,
            .tranche_target_device = &OutputSurface::dmabufFeedbackTrancheTargetDevice,
            .tranche_formats = &OutputSurface::dmabufFeedbackTrancheFormats,
            .tranche_flags = &OutputSurface::dmabufFeedbackTrancheFlags,
        };
        zwp_linux_dmabuf_feedback_v1_add_listener(
            m_surfaceDmabufFeedback,
            &feedbackListener,
            this
        );
    }

    wl_surface_commit(m_surface);
    flush();
}

void OutputSurface::destroySurface() {
    if (m_frameCallback != nullptr) {
        wl_callback_destroy(m_frameCallback);
        m_frameCallback = nullptr;
    }
    destroyBuffers();
    if (m_surfaceDmabufFeedback != nullptr) {
        zwp_linux_dmabuf_feedback_v1_destroy(m_surfaceDmabufFeedback);
        m_surfaceDmabufFeedback = nullptr;
    }
    m_dmabufFormatTable.clear();
    m_surfaceDmabufFormats.clear();
    m_dmabufFeedbackReady = false;
    m_dmabufDisabled = false;
    if (m_layerSurface != nullptr) {
        zwlr_layer_surface_v1_destroy(m_layerSurface);
        m_layerSurface = nullptr;
    }
    if (m_surface != nullptr) {
        wl_surface_destroy(m_surface);
        m_surface = nullptr;
    }
    m_configured = false;
    m_dirty = false;
}

void OutputSurface::applySnapshot(
    const SnapshotModel &snapshot,
    const std::map<QString, std::shared_ptr<SourceSession>> &sources
) {
    const auto presentBackend = m_renderer->resolvePresentBackend(snapshot.presentBackend);
    m_requestedPresentBackend = presentBackend.requested;
    m_resolvedPresentBackend = presentBackend.resolved;
    m_presentBackendFallbackReason = presentBackend.fallbackReason;
    if (m_requestedPresentBackend == QLatin1String("shm")) {
        destroyDmabufBuffers();
        m_dmabufDisabled = false;
    }

    std::shared_ptr<SourceSession> nextSource;
    std::optional<CropRect> nextCrop;

    const auto viewIt = snapshot.views.find(m_outputName);
    if (viewIt != snapshot.views.end()) {
        const auto sourceIt = sources.find(viewIt->source);
        if (sourceIt != sources.end()) {
            nextSource = sourceIt->second;
            nextCrop = viewIt->crop;
        }
    } else if (!snapshot.fallbackSource.isEmpty()) {
        const auto sourceIt = sources.find(snapshot.fallbackSource);
        if (sourceIt != sources.end()) {
            nextSource = sourceIt->second;
        }
    }

    setBinding(std::move(nextSource), nextCrop, snapshot.transitionDurationMs);
}

void OutputSurface::scheduleRender() {
    m_dirty = true;
    if (m_surface == nullptr || !m_configured) {
        return;
    }
    if (m_frameCallback == nullptr) {
        render();
    }
}

void OutputSurface::onFrameCallbackDone() {
    m_presentedFrames += 1;
    if (m_frameCallback != nullptr) {
        wl_callback_destroy(m_frameCallback);
        m_frameCallback = nullptr;
    }
    if (m_dirty) {
        render();
    }
}

void OutputSurface::onBufferReleased() {
    if (m_dirty && m_frameCallback == nullptr) {
        render();
    }
}

void OutputSurface::handleClosed() {
    qWarning().noquote() << kLogPrefix << "layer surface closed" << m_outputName;
    destroySurface();
}

void OutputSurface::handleConfigure(uint32_t serial, uint32_t width, uint32_t height) {
    if (m_layerSurface == nullptr) {
        return;
    }
    zwlr_layer_surface_v1_ack_configure(m_layerSurface, serial);

    if (width > 0 && height > 0) {
        m_logicalSize = QSize(static_cast<int>(width), static_cast<int>(height));
        m_pixelSize = QSize(m_logicalSize.width() * m_scale, m_logicalSize.height() * m_scale);
        destroyBuffers();
        updateVideoHint();
    }

    m_configured = true;
    scheduleRender();
}

void OutputSurface::updateVideoHint() {
    if (m_source != nullptr) {
        m_source->updateRenderHint(this, m_pixelSize);
    }
}

void OutputSurface::setBinding(std::shared_ptr<SourceSession> source, std::optional<CropRect> crop, int transitionMs) {
    const bool sourceChanged = m_source.get() != source.get();
    const bool cropChanged = m_crop != crop;
    if (!sourceChanged && !cropChanged) {
        return;
    }

    if (sourceChanged) {
        capturePreviousImage();
        detachCurrentSourceHint();
        m_source = std::move(source);
        updateVideoHint();
    } else {
        m_source = std::move(source);
    }

    m_crop = crop;
    if (sourceChanged && transitionMs > 0) {
        startTransition(transitionMs);
    } else if (sourceChanged) {
        stopTransition();
    }
    scheduleRender();
}

void OutputSurface::detachCurrentSourceHint() {
    if (m_source != nullptr) {
        m_source->removeRenderHint(this);
    }
}

void OutputSurface::ensureShmBuffers() {
    if (m_pixelSize.isEmpty() || m_renderer->shm() == nullptr) {
        return;
    }

    const int expectedWidth = m_pixelSize.width();
    const int expectedHeight = m_pixelSize.height();
    for (const auto &buffer : m_shmBuffers) {
        if (buffer.buffer == nullptr ||
            buffer.width != expectedWidth ||
            buffer.height != expectedHeight) {
            destroyShmBuffers();
            break;
        }
    }

    for (auto &buffer : m_shmBuffers) {
        if (buffer.buffer != nullptr) {
            continue;
        }

        buffer.width = expectedWidth;
        buffer.height = expectedHeight;
        buffer.stride = expectedWidth * 4;
        buffer.bytes = static_cast<size_t>(buffer.stride) * expectedHeight;
        buffer.owner = this;

        const int fd = ::memfd_create("qsov-wallpaper-buffer", MFD_CLOEXEC);
        if (fd < 0) {
            qWarning().noquote() << kLogPrefix << "memfd_create failed";
            destroyShmBuffers();
            return;
        }

        if (::ftruncate(fd, static_cast<off_t>(buffer.bytes)) < 0) {
            qWarning().noquote() << kLogPrefix << "ftruncate failed";
            ::close(fd);
            destroyShmBuffers();
            return;
        }

        buffer.data = ::mmap(nullptr, buffer.bytes, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
        if (buffer.data == MAP_FAILED) {
            qWarning().noquote() << kLogPrefix << "mmap failed";
            buffer.data = nullptr;
            ::close(fd);
            destroyShmBuffers();
            return;
        }

        wl_shm_pool *pool = wl_shm_create_pool(
            m_renderer->shm(),
            fd,
            static_cast<int>(buffer.bytes)
        );
        buffer.buffer = wl_shm_pool_create_buffer(
            pool,
            0,
            buffer.width,
            buffer.height,
            buffer.stride,
            WL_SHM_FORMAT_ARGB8888
        );
        wl_shm_pool_destroy(pool);
        ::close(fd);

        static constexpr wl_buffer_listener bufferListener = {
            .release = &OutputSurface::shmBufferReleased,
        };
        wl_buffer_add_listener(buffer.buffer, &bufferListener, &buffer);
    }
}

bool OutputSurface::supportsDmabufModifier(uint32_t format, uint64_t modifier) const {
    return std::any_of(
        m_surfaceDmabufFormats.cbegin(),
        m_surfaceDmabufFormats.cend(),
        [format, modifier](const DmabufFormatModifier &entry) {
            return entry.format == format && entry.modifier == modifier;
        }
    );
}

std::vector<uint64_t> OutputSurface::supportedDmabufModifiers(uint32_t format) const {
    std::vector<uint64_t> modifiers;
    for (const auto &entry : m_surfaceDmabufFormats) {
        if (entry.format != format) {
            continue;
        }
        if (std::find(modifiers.cbegin(), modifiers.cend(), entry.modifier) == modifiers.cend()) {
            modifiers.push_back(entry.modifier);
        }
    }

    std::sort(modifiers.begin(), modifiers.end(), [](uint64_t left, uint64_t right) {
        auto rank = [](uint64_t modifier) {
            if (modifier == DRM_FORMAT_MOD_LINEAR) {
                return 0;
            }
            if (modifier == DRM_FORMAT_MOD_INVALID) {
                return 1;
            }
            return 2;
        };
        if (rank(left) != rank(right)) {
            return rank(left) < rank(right);
        }
        return left < right;
    });

    return modifiers;
}

bool OutputSurface::createDmabufBuffer(DmaBuffer &buffer) {
    QString reason;
    if (!m_renderer->ensureGbmDevice(&reason) || m_renderer->gbmDevice() == nullptr) {
        m_presentBackendFallbackReason = reason.isEmpty()
            ? QStringLiteral("gbm_device_unavailable")
            : reason;
        return false;
    }

    if (m_renderer->dmabufVersion() >= 4 && !m_dmabufFeedbackReady) {
        m_presentBackendFallbackReason = QStringLiteral("dmabuf_feedback_pending");
        return false;
    }

    std::vector<uint64_t> modifiers = supportedDmabufModifiers(kDmabufDrmFormat);
    if (m_renderer->dmabufVersion() >= 4 && modifiers.empty()) {
        m_presentBackendFallbackReason = QStringLiteral("dmabuf_format_unsupported");
        return false;
    }
    if (modifiers.empty()) {
        modifiers = {DRM_FORMAT_MOD_LINEAR, DRM_FORMAT_MOD_INVALID};
    }

    gbm_bo *bo = nullptr;
    uint64_t protocolModifier = DRM_FORMAT_MOD_INVALID;
    for (const uint64_t advertisedModifier : modifiers) {
        if (advertisedModifier == DRM_FORMAT_MOD_INVALID) {
            bo = gbm_bo_create(
                m_renderer->gbmDevice(),
                static_cast<uint32_t>(m_pixelSize.width()),
                static_cast<uint32_t>(m_pixelSize.height()),
                kDmabufDrmFormat,
                GBM_BO_USE_RENDERING | GBM_BO_USE_LINEAR
            );
            protocolModifier = DRM_FORMAT_MOD_INVALID;
        } else {
            const uint64_t modifier = advertisedModifier;
            bo = gbm_bo_create_with_modifiers2(
                m_renderer->gbmDevice(),
                static_cast<uint32_t>(m_pixelSize.width()),
                static_cast<uint32_t>(m_pixelSize.height()),
                kDmabufDrmFormat,
                &modifier,
                1,
                GBM_BO_USE_RENDERING
            );
            protocolModifier = modifier;
        }

        if (bo == nullptr) {
            continue;
        }

        if (gbm_bo_get_plane_count(bo) != 1) {
            gbm_bo_destroy(bo);
            bo = nullptr;
            continue;
        }

        if (protocolModifier != DRM_FORMAT_MOD_INVALID) {
            const uint64_t actualModifier = gbm_bo_get_modifier(bo);
            if (!supportsDmabufModifier(kDmabufDrmFormat, actualModifier)) {
                gbm_bo_destroy(bo);
                bo = nullptr;
                continue;
            }
            protocolModifier = actualModifier;
        }

        break;
    }

    if (bo == nullptr) {
        m_presentBackendFallbackReason = QStringLiteral("gbm_bo_create_failed");
        return false;
    }

    uint32_t planeStride = gbm_bo_get_stride_for_plane(bo, 0);
    if (planeStride == 0) {
        planeStride = gbm_bo_get_stride(bo);
    }
    const uint32_t planeOffset = gbm_bo_get_offset(bo, 0);
    int fd = gbm_bo_get_fd_for_plane(bo, 0);
    if (fd < 0) {
        fd = gbm_bo_get_fd(bo);
    }
    if (fd < 0) {
        gbm_bo_destroy(bo);
        m_presentBackendFallbackReason = QStringLiteral("gbm_bo_export_failed");
        return false;
    }

    zwp_linux_buffer_params_v1 *params =
        zwp_linux_dmabuf_v1_create_params(m_renderer->linuxDmabuf());
    if (params == nullptr) {
        ::close(fd);
        gbm_bo_destroy(bo);
        m_presentBackendFallbackReason = QStringLiteral("dmabuf_params_create_failed");
        return false;
    }

    static constexpr zwp_linux_buffer_params_v1_listener paramsListener = {
        .created = &OutputSurface::dmabufParamsCreated,
        .failed = &OutputSurface::dmabufParamsFailed,
    };
    zwp_linux_buffer_params_v1_add_listener(params, &paramsListener, &buffer);
    zwp_linux_buffer_params_v1_add(
        params,
        fd,
        0,
        planeOffset,
        planeStride,
        modifierHi(protocolModifier),
        modifierLo(protocolModifier)
    );
    ::close(fd);
    zwp_linux_buffer_params_v1_create(
        params,
        m_pixelSize.width(),
        m_pixelSize.height(),
        kDmabufDrmFormat,
        0
    );

    buffer.owner = this;
    buffer.params = params;
    buffer.bo = bo;
    buffer.data = nullptr;
    buffer.mapData = nullptr;
    buffer.width = m_pixelSize.width();
    buffer.height = m_pixelSize.height();
    buffer.stride = static_cast<int>(planeStride);
    buffer.format = kDmabufDrmFormat;
    buffer.modifier = protocolModifier;
    buffer.pending = true;
    buffer.busy = false;
    flush();

    qInfo().noquote() << kLogPrefix
                      << "dmabuf buffer requested"
                      << m_outputName
                      << drmFormatString(buffer.format)
                      << dmabufModifierString(buffer.modifier);
    return true;
}

bool OutputSurface::ensureDmabufBuffers() {
    if (m_requestedPresentBackend == QLatin1String("shm") ||
        m_pixelSize.isEmpty() ||
        m_renderer->linuxDmabuf() == nullptr ||
        m_dmabufDisabled) {
        return false;
    }

    const int expectedWidth = m_pixelSize.width();
    const int expectedHeight = m_pixelSize.height();
    for (const auto &buffer : m_dmabufBuffers) {
        if ((buffer.buffer != nullptr || buffer.pending) &&
            (buffer.width != expectedWidth || buffer.height != expectedHeight)) {
            destroyDmabufBuffers();
            break;
        }
    }

    bool anyReady = false;
    for (auto &buffer : m_dmabufBuffers) {
        if (buffer.buffer != nullptr) {
            anyReady = true;
            continue;
        }
        if (buffer.pending) {
            continue;
        }
        if (!createDmabufBuffer(buffer)) {
            return anyReady;
        }
    }

    return anyReady;
}

void OutputSurface::releaseDmabufBuffer(DmaBuffer &buffer) {
    if (buffer.buffer != nullptr) {
        wl_buffer_destroy(buffer.buffer);
        buffer.buffer = nullptr;
    }
    if (buffer.params != nullptr) {
        zwp_linux_buffer_params_v1_destroy(buffer.params);
        buffer.params = nullptr;
    }
    if (buffer.bo != nullptr && buffer.mapData != nullptr) {
        gbm_bo_unmap(buffer.bo, buffer.mapData);
        buffer.mapData = nullptr;
    }
    if (buffer.bo != nullptr) {
        gbm_bo_destroy(buffer.bo);
        buffer.bo = nullptr;
    }
    buffer.data = nullptr;
    buffer.width = 0;
    buffer.height = 0;
    buffer.stride = 0;
    buffer.modifier = DRM_FORMAT_MOD_INVALID;
    buffer.busy = false;
    buffer.pending = false;
}

void OutputSurface::destroyShmBuffers() {
    for (auto &buffer : m_shmBuffers) {
        if (buffer.buffer != nullptr) {
            wl_buffer_destroy(buffer.buffer);
            buffer.buffer = nullptr;
        }
        if (buffer.data != nullptr) {
            ::munmap(buffer.data, buffer.bytes);
            buffer.data = nullptr;
        }
        buffer.bytes = 0;
        buffer.width = 0;
        buffer.height = 0;
        buffer.stride = 0;
        buffer.busy = false;
    }
}

void OutputSurface::destroyDmabufBuffers() {
    for (auto &buffer : m_dmabufBuffers) {
        releaseDmabufBuffer(buffer);
    }
}

void OutputSurface::destroyBuffers() {
    m_lastPresentedIndex = -1;
    m_lastPresentedBackend = QStringLiteral("shm");
    destroyDmabufBuffers();
    destroyShmBuffers();
}

OutputSurface::ShmBuffer *OutputSurface::nextFreeShmBuffer() {
    for (auto &buffer : m_shmBuffers) {
        if (!buffer.busy && buffer.buffer != nullptr) {
            return &buffer;
        }
    }
    return nullptr;
}

OutputSurface::DmaBuffer *OutputSurface::nextFreeDmabufBuffer() {
    for (auto &buffer : m_dmabufBuffers) {
        if (!buffer.busy && !buffer.pending && buffer.buffer != nullptr) {
            return &buffer;
        }
    }
    return nullptr;
}

void OutputSurface::disableDmabuf(const QString &reason) {
    m_dmabufDisabled = true;
    m_presentBackendFallbackReason = reason;
    m_resolvedPresentBackend = QStringLiteral("shm");
    destroyDmabufBuffers();
}

void OutputSurface::render() {
    if (!m_configured || m_surface == nullptr || m_pixelSize.isEmpty()) {
        return;
    }

    wl_buffer *wlBuffer = nullptr;
    void *data = nullptr;
    int width = 0;
    int height = 0;
    int stride = 0;
    int bufferIndex = -1;
    QString usedBackend = QStringLiteral("shm");
    DmaBuffer *usedDmabuf = nullptr;
    ShmBuffer *usedShm = nullptr;

    if (ensureDmabufBuffers()) {
        if (DmaBuffer *buffer = nextFreeDmabufBuffer(); buffer != nullptr) {
            buffer->busy = true;
            wlBuffer = buffer->buffer;
            width = buffer->width;
            height = buffer->height;
            bufferIndex = static_cast<int>(buffer - m_dmabufBuffers.data());
            usedBackend = QStringLiteral("dmabuf");
            usedDmabuf = buffer;
        }
    }

    if (wlBuffer == nullptr) {
        ensureShmBuffers();
        if (ShmBuffer *buffer = nextFreeShmBuffer(); buffer != nullptr) {
            buffer->busy = true;
            wlBuffer = buffer->buffer;
            data = buffer->data;
            width = buffer->width;
            height = buffer->height;
            stride = buffer->stride;
            bufferIndex = static_cast<int>(buffer - m_shmBuffers.data());
            usedBackend = QStringLiteral("shm");
            usedShm = buffer;
        }
    }

    if (usedDmabuf != nullptr) {
        uint32_t mappedStride = 0;
        void *mapData = nullptr;
        void *mapped = gbm_bo_map(
            usedDmabuf->bo,
            0,
            0,
            static_cast<uint32_t>(usedDmabuf->width),
            static_cast<uint32_t>(usedDmabuf->height),
            GBM_BO_TRANSFER_WRITE,
            &mappedStride,
            &mapData
        );
        if (mapped == nullptr || mapped == MAP_FAILED) {
            usedDmabuf->busy = false;
            disableDmabuf(QStringLiteral("gbm_bo_map_failed"));
            wlBuffer = nullptr;
            usedDmabuf = nullptr;
            usedBackend = QStringLiteral("shm");
        } else {
            usedDmabuf->data = mapped;
            usedDmabuf->mapData = mapData;
            usedDmabuf->stride = static_cast<int>(mappedStride);
            data = mapped;
            stride = usedDmabuf->stride;
        }
    }

    if (wlBuffer == nullptr) {
        ensureShmBuffers();
        if (usedShm == nullptr) {
            usedShm = nextFreeShmBuffer();
            if (usedShm != nullptr) {
                usedShm->busy = true;
                wlBuffer = usedShm->buffer;
                data = usedShm->data;
                width = usedShm->width;
                height = usedShm->height;
                stride = usedShm->stride;
                bufferIndex = static_cast<int>(usedShm - m_shmBuffers.data());
                usedBackend = QStringLiteral("shm");
            }
        }
    }

    if (wlBuffer == nullptr || data == nullptr) {
        m_bufferStarvedFrames += 1;
        return;
    }

    QImage target(
        static_cast<uchar *>(data),
        width,
        height,
        stride,
        QImage::Format_ARGB32_Premultiplied
    );
    target.fill(Qt::black);

    QPainter painter(&target);
    painter.setCompositionMode(QPainter::CompositionMode_SourceOver);

    qreal transitionProgress = 1.0;
    if (m_transitionDurationMs > 0 && m_transitionClock.isValid()) {
        transitionProgress = std::clamp(
            static_cast<qreal>(m_transitionClock.elapsed()) / m_transitionDurationMs,
            0.0,
            1.0
        );
        if (transitionProgress >= 1.0) {
            stopTransition();
        }
    }

    if (!m_previousImage.isNull() && transitionProgress < 1.0) {
        paintImageCover(painter, m_previousImage, m_pixelSize, std::nullopt, 1.0 - transitionProgress);
    }

    if (m_source != nullptr) {
        const qreal opacity = (!m_previousImage.isNull() && transitionProgress < 1.0)
            ? transitionProgress
            : 1.0;
        m_source->paint(painter, m_pixelSize, m_crop, opacity);
    }

    painter.end();

    if (usedDmabuf != nullptr && usedDmabuf->mapData != nullptr) {
        gbm_bo_unmap(usedDmabuf->bo, usedDmabuf->mapData);
        usedDmabuf->mapData = nullptr;
        usedDmabuf->data = nullptr;
    }

    wl_surface_set_buffer_scale(m_surface, m_scale);
    wl_surface_attach(m_surface, wlBuffer, 0, 0);
    wl_surface_damage_buffer(m_surface, 0, 0, width, height);
    if (m_frameCallback != nullptr) {
        wl_callback_destroy(m_frameCallback);
        m_frameCallback = nullptr;
    }
    m_frameCallback = wl_surface_frame(m_surface);
    static constexpr wl_callback_listener callbackListener = {
        .done = &OutputSurface::frameDone,
    };
    wl_callback_add_listener(m_frameCallback, &callbackListener, this);
    wl_surface_commit(m_surface);
    flush();

    m_committedFrames += 1;
    m_lastPresentedIndex = bufferIndex;
    m_lastPresentedBackend = usedBackend;
    m_resolvedPresentBackend = usedBackend;
    if (usedBackend == QLatin1String("dmabuf")) {
        m_presentBackendFallbackReason.clear();
    } else if (m_requestedPresentBackend != QLatin1String("shm") &&
               m_presentBackendFallbackReason.isEmpty()) {
        m_presentBackendFallbackReason = m_dmabufFeedbackReady
            ? QStringLiteral("dmabuf_buffer_pending")
            : QStringLiteral("dmabuf_feedback_pending");
    }
    m_dirty = false;
}

void OutputSurface::startTransition(int durationMs) {
    if (m_previousImage.isNull()) {
        return;
    }
    m_transitionDurationMs = std::max(durationMs, 0);
    if (m_transitionDurationMs <= 0) {
        return;
    }
    m_transitionClock.restart();
    if (!m_transitionTimer.isActive()) {
        m_transitionTimer.start();
    }
}

void OutputSurface::stopTransition() {
    m_transitionDurationMs = 0;
    m_previousImage = QImage();
    m_transitionClock.invalidate();
    m_transitionTimer.stop();
}

void OutputSurface::capturePreviousImage() {
    if (m_lastPresentedIndex < 0 || m_lastPresentedIndex >= static_cast<int>(m_shmBuffers.size())) {
        m_previousImage = QImage();
        return;
    }

    const uchar *data = nullptr;
    int width = 0;
    int height = 0;
    int stride = 0;

    if (m_lastPresentedBackend == QLatin1String("dmabuf")) {
        const DmaBuffer &buffer = m_dmabufBuffers[static_cast<size_t>(m_lastPresentedIndex)];
        data = static_cast<const uchar *>(buffer.data);
        width = buffer.width;
        height = buffer.height;
        stride = buffer.stride;
    } else {
        const ShmBuffer &buffer = m_shmBuffers[static_cast<size_t>(m_lastPresentedIndex)];
        data = static_cast<const uchar *>(buffer.data);
        width = buffer.width;
        height = buffer.height;
        stride = buffer.stride;
    }

    if (data == nullptr || width <= 0 || height <= 0) {
        m_previousImage = QImage();
        return;
    }

    const QImage current(data, width, height, stride, QImage::Format_ARGB32_Premultiplied);
    m_previousImage = current.copy();
}

void OutputSurface::flush() {
    m_renderer->flush();
}

void OutputSurface::layerSurfaceConfigure(
    void *data,
    zwlr_layer_surface_v1 *,
    uint32_t serial,
    uint32_t width,
    uint32_t height
) {
    static_cast<OutputSurface *>(data)->handleConfigure(serial, width, height);
}

void OutputSurface::layerSurfaceClosed(void *data, zwlr_layer_surface_v1 *) {
    static_cast<OutputSurface *>(data)->handleClosed();
}

void OutputSurface::frameDone(void *data, wl_callback *, uint32_t) {
    static_cast<OutputSurface *>(data)->onFrameCallbackDone();
}

void OutputSurface::shmBufferReleased(void *data, wl_buffer *) {
    auto *buffer = static_cast<ShmBuffer *>(data);
    buffer->busy = false;
    if (buffer->owner != nullptr) {
        buffer->owner->onBufferReleased();
    }
}

void OutputSurface::dmabufBufferReleased(void *data, wl_buffer *) {
    auto *buffer = static_cast<DmaBuffer *>(data);
    buffer->busy = false;
    if (buffer->owner != nullptr) {
        buffer->owner->onBufferReleased();
    }
}

void OutputSurface::dmabufParamsCreated(
    void *data,
    zwp_linux_buffer_params_v1 *params,
    wl_buffer *buffer
) {
    auto *dmabuf = static_cast<DmaBuffer *>(data);
    dmabuf->params = nullptr;
    dmabuf->buffer = buffer;
    dmabuf->pending = false;
    zwp_linux_buffer_params_v1_destroy(params);

    static constexpr wl_buffer_listener bufferListener = {
        .release = &OutputSurface::dmabufBufferReleased,
    };
    wl_buffer_add_listener(buffer, &bufferListener, dmabuf);

    if (dmabuf->owner != nullptr) {
        dmabuf->owner->onDmabufBufferCreated();
    }
}

void OutputSurface::dmabufParamsFailed(void *data, zwp_linux_buffer_params_v1 *params) {
    auto *dmabuf = static_cast<DmaBuffer *>(data);
    dmabuf->params = nullptr;
    dmabuf->pending = false;
    zwp_linux_buffer_params_v1_destroy(params);

    if (dmabuf->owner != nullptr) {
        dmabuf->owner->onDmabufBufferFailed(dmabuf);
    }
}

void OutputSurface::dmabufFeedbackDone(void *data, zwp_linux_dmabuf_feedback_v1 *) {
    auto *self = static_cast<OutputSurface *>(data);
    self->m_dmabufFeedbackReady = true;
    self->scheduleRender();
}

void OutputSurface::dmabufFeedbackFormatTable(
    void *data,
    zwp_linux_dmabuf_feedback_v1 *,
    int32_t fd,
    uint32_t size
) {
    auto *self = static_cast<OutputSurface *>(data);
    self->m_dmabufFormatTable.clear();
    self->m_surfaceDmabufFormats.clear();

    if (fd < 0 || size == 0) {
        if (fd >= 0) {
            ::close(fd);
        }
        return;
    }

    void *mapped = ::mmap(nullptr, size, PROT_READ, MAP_PRIVATE, fd, 0);
    if (mapped != MAP_FAILED) {
        self->m_dmabufFormatTable = QByteArray(static_cast<const char *>(mapped), static_cast<int>(size));
        ::munmap(mapped, size);
    }
    ::close(fd);
}

void OutputSurface::dmabufFeedbackMainDevice(
    void *,
    zwp_linux_dmabuf_feedback_v1 *,
    wl_array *
) {}

void OutputSurface::dmabufFeedbackTrancheDone(void *, zwp_linux_dmabuf_feedback_v1 *) {}

void OutputSurface::dmabufFeedbackTrancheTargetDevice(
    void *,
    zwp_linux_dmabuf_feedback_v1 *,
    wl_array *
) {}

void OutputSurface::dmabufFeedbackTrancheFormats(
    void *data,
    zwp_linux_dmabuf_feedback_v1 *,
    wl_array *indices
) {
    auto *self = static_cast<OutputSurface *>(data);
    if (indices == nullptr || indices->data == nullptr || self->m_dmabufFormatTable.isEmpty()) {
        return;
    }

    const auto *table = reinterpret_cast<const unsigned char *>(self->m_dmabufFormatTable.constData());
    const size_t tableSize = static_cast<size_t>(self->m_dmabufFormatTable.size());
    const auto *begin = static_cast<const uint16_t *>(indices->data);
    const auto count = indices->size / sizeof(uint16_t);

    for (size_t i = 0; i < count; ++i) {
        const size_t offset = static_cast<size_t>(begin[i]) * 16;
        if (offset + 16 > tableSize) {
            continue;
        }

        uint32_t format = 0;
        uint64_t modifier = DRM_FORMAT_MOD_INVALID;
        std::memcpy(&format, table + offset, sizeof(uint32_t));
        std::memcpy(&modifier, table + offset + 8, sizeof(uint64_t));

        const DmabufFormatModifier entry{
            .format = format,
            .modifier = modifier,
        };
        if (std::find(
                self->m_surfaceDmabufFormats.cbegin(),
                self->m_surfaceDmabufFormats.cend(),
                entry
            ) == self->m_surfaceDmabufFormats.cend()) {
            self->m_surfaceDmabufFormats.push_back(entry);
        }
    }
}

void OutputSurface::dmabufFeedbackTrancheFlags(
    void *,
    zwp_linux_dmabuf_feedback_v1 *,
    uint32_t
) {}

void OutputSurface::onDmabufBufferCreated() {
    scheduleRender();
}

void OutputSurface::onDmabufBufferFailed(DmaBuffer *buffer) {
    if (buffer == nullptr) {
        return;
    }

    qWarning().noquote() << kLogPrefix
                         << "dmabuf buffer import failed"
                         << m_outputName
                         << drmFormatString(buffer->format)
                         << dmabufModifierString(buffer->modifier);
    releaseDmabufBuffer(*buffer);
    disableDmabuf(QStringLiteral("dmabuf_import_failed"));
    scheduleRender();
}

wl_output *OutputSurface::output() const {
    return m_output;
}

class WallpaperProtocolClient final : public QObject {
    Q_OBJECT

public:
    explicit WallpaperProtocolClient(QString socketPath, QObject *parent = nullptr)
        : QObject(parent)
        , m_socketPath(std::move(socketPath)) {
        connect(&m_socket, &QLocalSocket::connected, this, &WallpaperProtocolClient::onConnected);
        connect(&m_socket, &QLocalSocket::readyRead, this, &WallpaperProtocolClient::onReadyRead);
        connect(&m_socket, &QLocalSocket::disconnected, this, [this]() {
            emit fatalError(QStringLiteral("daemon socket disconnected"));
        });
        connect(
            &m_socket,
            &QLocalSocket::errorOccurred,
            this,
            [this](QLocalSocket::LocalSocketError) {
                emit fatalError(m_socket.errorString());
            }
        );
    }

    void start() {
        qInfo().noquote() << kLogPrefix << "connecting to daemon socket" << m_socketPath;
        m_socket.connectToServer(m_socketPath);
    }

signals:
    void snapshotReceived(const QJsonObject &payload);
    void fatalError(const QString &message);

private:
    void sendJson(const QJsonObject &object) {
        const QByteArray encoded = QJsonDocument(object).toJson(QJsonDocument::Compact) + '\n';
        m_socket.write(encoded);
        m_socket.flush();
    }

    void onConnected() {
        sendJson(QJsonObject{
            {QStringLiteral("proto_version"), QStringLiteral("qsov/1")},
            {QStringLiteral("client_name"), QStringLiteral("qsov-wallpaper-native")},
            {QStringLiteral("client_version"), QStringLiteral("0.1")},
        });
    }

    void onReadyRead() {
        m_buffer += m_socket.readAll();

        qsizetype newline = 0;
        while ((newline = m_buffer.indexOf('\n')) >= 0) {
            const QByteArray line = m_buffer.left(newline).trimmed();
            m_buffer.remove(0, newline + 1);
            if (line.isEmpty()) {
                continue;
            }

            const QJsonDocument doc = QJsonDocument::fromJson(line);
            if (!doc.isObject()) {
                emit fatalError(QStringLiteral("received malformed daemon JSON"));
                return;
            }

            const QJsonObject object = doc.object();
            if (object.value(QStringLiteral("_type")).toString() == QStringLiteral("HelloAck")) {
                sendJson(QJsonObject{
                    {QStringLiteral("id"), 0},
                    {QStringLiteral("kind"), 5},
                    {QStringLiteral("topic"), QStringLiteral("wallpaper")},
                    {QStringLiteral("action"), QStringLiteral("")},
                    {QStringLiteral("payload"), QJsonValue::Null},
                });
                continue;
            }

            const int kind = object.value(QStringLiteral("kind")).toInt(-1);
            const QString topic = object.value(QStringLiteral("topic")).toString();
            if (kind == 3 && topic == QStringLiteral("wallpaper")) {
                emit snapshotReceived(object.value(QStringLiteral("payload")).toObject());
            } else if (kind == 2) {
                emit fatalError(QStringLiteral("daemon returned ERR for wallpaper subscription"));
            }
        }
    }

    QString m_socketPath;
    QLocalSocket m_socket;
    QByteArray m_buffer;
};

class WallpaperRuntime final : public QObject {
    Q_OBJECT

public:
    explicit WallpaperRuntime(QObject *parent = nullptr)
        : QObject(parent)
        , m_protocol(defaultSocketPath(), this) {
        connect(&m_protocol, &WallpaperProtocolClient::snapshotReceived, this, [this](const QJsonObject &payload) {
            m_renderer.applySnapshot(parseSnapshot(payload));
        });
        connect(&m_protocol, &WallpaperProtocolClient::fatalError, this, &WallpaperRuntime::fail);
        connect(&m_renderer, &WaylandRenderer::fatalError, this, &WallpaperRuntime::fail);
    }

    int start() {
        QString error;
        if (!m_renderer.initialize(&error)) {
            fail(error);
            return 1;
        }

        m_protocol.start();
        return 0;
    }

private:
    void fail(const QString &message) {
        qCritical().noquote() << kLogPrefix << message;
        QCoreApplication::exit(1);
    }

    WaylandRenderer m_renderer;
    WallpaperProtocolClient m_protocol;
};

} // namespace

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("qsov-wallpaper-native"));

    WallpaperRuntime runtime;
    const int startup = runtime.start();
    if (startup != 0) {
        return startup;
    }

    return app.exec();
}

#include "main.moc"
