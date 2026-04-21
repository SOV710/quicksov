// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "../../wallpaper_ffmpeg/src/WallpaperVideo.hpp"

#include <array>
#include <map>
#include <memory>
#include <optional>
#include <vector>

#include <sys/types.h>

#include <QByteArray>
#include <QElapsedTimer>
#include <QHash>
#include <QImage>
#include <QJsonValue>
#include <QJsonObject>
#include <QLocalSocket>
#include <QObject>
#include <QPainter>
#include <QPointer>
#include <QRectF>
#include <QSize>
#include <QSocketNotifier>
#include <QStringList>
#include <QTimer>

extern "C" {
#include <EGL/egl.h>
#include <gbm.h>
#include <drm_fourcc.h>
#include <libavutil/frame.h>
#include <wayland-client.h>
}

#define PL_LIBAV_IMPLEMENTATION 0
#include <libplacebo/colorspace.h>
#include <libplacebo/common.h>
#include <libplacebo/gpu.h>
#include <libplacebo/log.h>
#include <libplacebo/opengl.h>
#include <libplacebo/renderer.h>
#include <libplacebo/utils/libav.h>

#define namespace namespace_
#include "wlr-layer-shell-unstable-v1-client-protocol.h"
#undef namespace
#include "linux-dmabuf-v1-client-protocol.h"

namespace quicksov::wallpaper_native {

inline constexpr const char *kLogPrefix = "[wallpaper-native]";
inline constexpr const char *kNamespace = "quicksov-wallpaper";
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

class GpuCompositor final {
public:
    ~GpuCompositor();

    bool initialize(gbm_device *gbmDevice, QString *error);
    void destroy();
    [[nodiscard]] bool available() const;
    void releaseTarget(quintptr key);
    bool renderToDmabuf(
        const WallpaperVideo::HardwareFrameSnapshot &source,
        const QSize &targetSize,
        const std::optional<CropRect> &crop,
        quintptr targetKey,
        int targetFd,
        int width,
        int height,
        int stride,
        int offset,
        uint32_t drmFormat,
        uint64_t modifier,
        bool conservativeSync,
        QString *error
    );

private:
    struct SourceFrame {
        quint64 serial = 0;
        WallpaperVideo::AvFramePtr frame;
        struct pl_frame image = {};
        pl_tex textures[4] = {};
    };

    struct TargetTexture {
        pl_tex texture = nullptr;
        int fd = -1;
        int width = 0;
        int height = 0;
        int stride = 0;
        int offset = 0;
        uint32_t drmFormat = 0;
        uint64_t modifier = DRM_FORMAT_MOD_INVALID;
    };

    static void logCallback(void *, enum pl_log_level level, const char *msg);
    static void cleanupEgl(EGLDisplay display, EGLSurface surface, EGLContext context);

    bool makeCurrent() const;
    void releaseSourceFrame();
    SourceFrame *ensureSourceFrame(
        const WallpaperVideo::HardwareFrameSnapshot &source,
        QString *error
    );
    TargetTexture *ensureTarget(
        quintptr key,
        int targetFd,
        int width,
        int height,
        int stride,
        int offset,
        uint32_t drmFormat,
        uint64_t modifier,
        QString *error
    );

    EGLDisplay m_display = EGL_NO_DISPLAY;
    EGLSurface m_surface = EGL_NO_SURFACE;
    EGLContext m_context = EGL_NO_CONTEXT;
    pl_log m_log = nullptr;
    pl_opengl m_opengl = nullptr;
    pl_renderer m_renderer = nullptr;
    SourceFrame m_sourceFrame;
    std::map<quintptr, TargetTexture> m_targets;
};

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

    explicit SourceSession(
        const SourceSpec &spec,
        const QStringList &decodeBackendOrder,
        const QString &preferredDevicePath,
        QObject *parent = nullptr
    );

    [[nodiscard]] const SourceSpec &spec() const;
    [[nodiscard]] bool isVideo() const;
    [[nodiscard]] bool ready() const;
    [[nodiscard]] bool matches(
        const SourceSpec &spec,
        const QStringList &decodeBackendOrder,
        const QString &preferredDevicePath
    ) const;
    [[nodiscard]] StatsSnapshot statsSnapshot() const;

    void updateRenderHint(QObject *owner, const QSize &size, bool cpuFrameRequired);
    void removeRenderHint(QObject *owner);
    [[nodiscard]] WallpaperVideo::HardwareFrameSnapshot hardwareFrameSnapshot() const;
    bool paint(
        QPainter &painter,
        const QSize &targetSize,
        const std::optional<CropRect> &crop,
        qreal opacity
    ) const;

signals:
    void updated();

private:
    SourceSpec m_spec;
    QStringList m_decodeBackendOrder;
    QString m_preferredDevicePath;
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
    void applySnapshot(
        const SnapshotModel &snapshot,
        const std::map<QString, std::shared_ptr<SourceSession>> &sources
    );
    void scheduleRender();
    void onFrameCallbackDone();
    void onBufferReleased();
    void handleClosed();
    void handleConfigure(uint32_t serial, uint32_t width, uint32_t height);
    void updateVideoHint();
    void resetGpuResources();

    [[nodiscard]] wl_output *output() const;

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
        int gpuImportFd = -1;
        void *data = nullptr;
        void *mapData = nullptr;
        int width = 0;
        int height = 0;
        int stride = 0;
        int offset = 0;
        uint32_t format = kDmabufDrmFormat;
        uint64_t modifier = DRM_FORMAT_MOD_INVALID;
        bool busy = false;
        bool pending = false;
    };

    void setBinding(std::shared_ptr<SourceSession> source, std::optional<CropRect> crop, int transitionMs);
    void detachCurrentSourceHint();
    void setCpuFrameRequired(bool required);
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
    std::vector<uint64_t> supportedDmabufModifiersForDevice(uint32_t format, dev_t targetDevice) const;
    std::vector<uint64_t> dmabufModifierCandidates(uint32_t format) const;
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
    QString m_targetPresentBackend = QStringLiteral("shm");
    QString m_activePresentBackend = QStringLiteral("shm");
    QString m_presentBackendFallbackReason;
    bool m_cpuFrameRequired = true;
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
    QString m_lastGpuError;
    bool m_loggedGpuFastPath = false;
    QElapsedTimer m_transitionClock;
    QTimer m_transitionTimer;
    int m_transitionDurationMs = 0;
    quint64 m_committedFrames = 0;
    quint64 m_presentedFrames = 0;
    quint64 m_bufferStarvedFrames = 0;
    QByteArray m_dmabufFormatTable;
    std::vector<DmabufFormatModifier> m_surfaceDmabufFormats;
    std::vector<DmabufTranche> m_surfaceDmabufTranches;
    DmabufTranche m_pendingDmabufTranche;
    QString m_lastDmabufAllocFailureSignature;
    quint64 m_dmabufAllocFailureRepeats = 0;
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

    explicit WaylandRenderer(QObject *parent = nullptr);
    ~WaylandRenderer() override;

    bool initialize(QString *error);
    void applySnapshot(const SnapshotModel &snapshot);

    [[nodiscard]] wl_compositor *compositor() const;
    [[nodiscard]] wl_shm *shm() const;
    [[nodiscard]] zwp_linux_dmabuf_v1 *linuxDmabuf() const;
    [[nodiscard]] GpuCompositor *gpuCompositor() const;
    [[nodiscard]] bool ensureGpuCompositor(QString *reason);
    [[nodiscard]] bool ensureGbmDevice(QString *reason);
    [[nodiscard]] gbm_device *gbmDevice() const;
    [[nodiscard]] bool ensureDmabufAllocationDevice(QString *reason);
    [[nodiscard]] gbm_device *dmabufAllocationDevice() const;
    [[nodiscard]] PresentBackendSelection resolvePresentBackend(const QString &requested) const;
    [[nodiscard]] bool dmabufAdvertised() const;
    [[nodiscard]] uint32_t dmabufVersion() const;
    [[nodiscard]] quint64 dmabufFormatCount() const;
    [[nodiscard]] quint64 dmabufModifierCount() const;
    [[nodiscard]] std::optional<dev_t> dmabufMainDevice() const;
    [[nodiscard]] QString gbmDevicePath() const;
    [[nodiscard]] QString dmabufAllocationDevicePath() const;
    [[nodiscard]] bool conservativeDmabufSyncEnabled() const;
    [[nodiscard]] zwlr_layer_shell_v1 *layerShell() const;
    void flush();
    void rebindOutputs();

signals:
    void fatalError(const QString &message);

private:
    [[nodiscard]] QVector<GpuDeviceInfo> gpuDevices() const;
    [[nodiscard]] QString compositorDevicePath() const;
    [[nodiscard]] QString resolveRenderDevicePath(const QVector<GpuDeviceInfo> &devices) const;
    [[nodiscard]] QString resolvePresentDevicePath(
        const QVector<GpuDeviceInfo> &devices,
        const QString &renderDevicePath
    ) const;
    [[nodiscard]] QString resolveDecodeDevicePath(
        const QVector<GpuDeviceInfo> &devices,
        const QString &renderDevicePath
    ) const;
    [[nodiscard]] QStringList resolveDecodeBackendOrder(
        const QVector<GpuDeviceInfo> &devices,
        const QString &decodeDevicePath
    ) const;
    [[nodiscard]] QString describeGpuPath(
        const QVector<GpuDeviceInfo> &devices,
        const QString &path
    ) const;
    void resetGpuPipeline();
    void logTelemetry();
    static void registryGlobal(
        void *data,
        wl_registry *registry,
        uint32_t name,
        const char *interface,
        uint32_t version
    );
    static void registryGlobalRemove(void *data, wl_registry *, uint32_t name);
    static void dmabufFormat(void *data, zwp_linux_dmabuf_v1 *, uint32_t);
    static void dmabufModifier(void *data, zwp_linux_dmabuf_v1 *, uint32_t, uint32_t, uint32_t);
    static void defaultFeedbackDone(void *, zwp_linux_dmabuf_feedback_v1 *);
    static void defaultFeedbackFormatTable(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        int32_t fd,
        uint32_t
    );
    static void defaultFeedbackMainDevice(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *,
        wl_array *device
    );
    static void defaultFeedbackTrancheDone(void *, zwp_linux_dmabuf_feedback_v1 *);
    static void defaultFeedbackTrancheTargetDevice(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        wl_array *
    );
    static void defaultFeedbackTrancheFormats(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        wl_array *
    );
    static void defaultFeedbackTrancheFlags(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        uint32_t
    );
    void onSourceUpdated(SourceSession *source);

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
    bool m_hasSnapshot = false;
    std::optional<dev_t> m_dmabufMainDevice;
    gbm_device *m_gbmDevice = nullptr;
    int m_gbmDeviceFd = -1;
    bool m_gbmDeviceAttempted = false;
    QString m_gbmDevicePath;
    QString m_gbmDeviceFailureReason;
    gbm_device *m_dmabufAllocationDevice = nullptr;
    int m_dmabufAllocationDeviceFd = -1;
    bool m_dmabufAllocationDeviceAttempted = false;
    QString m_dmabufAllocationDevicePath;
    QString m_dmabufAllocationDeviceFailureReason;
    std::unique_ptr<GpuCompositor> m_gpuCompositor;
    bool m_gpuCompositorAttempted = false;
    QString m_gpuCompositorFailureReason;
    QString m_effectiveCompositorDevicePath;
    QString m_effectiveRenderDevicePath;
    QString m_effectivePresentDevicePath;
    QString m_effectiveDecodeDevicePath;
    QStringList m_effectiveDecodeBackendOrder;
};

class WallpaperProtocolClient final : public QObject {
    Q_OBJECT

public:
    explicit WallpaperProtocolClient(QString socketPath, QObject *parent = nullptr);

    void start();

signals:
    void snapshotReceived(const QJsonObject &payload);
    void fatalError(const QString &message);

private:
    void sendJson(const QJsonObject &object);
    void onConnected();
    void onReadyRead();

    QString m_socketPath;
    QLocalSocket m_socket;
    QByteArray m_buffer;
};

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

} // namespace quicksov::wallpaper_native
