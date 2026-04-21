// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperVideo.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <dlfcn.h>
#include <memory>
#include <optional>

#include <QDebug>
#include <QFile>
#include <QFileInfo>
#include <QMetaObject>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
#include <libavutil/pixdesc.h>
#include <libavutil/imgutils.h>
#include <libswscale/swscale.h>
}

namespace {

QString ffmpegErrorString(int code) {
    char buffer[AV_ERROR_MAX_STRING_SIZE] = {};
    av_strerror(code, buffer, sizeof(buffer));
    return QString::fromUtf8(buffer);
}

QString pixelFormatName(AVPixelFormat format) {
    const char *name = av_get_pix_fmt_name(format);
    if (name == nullptr) {
        return QStringLiteral("unknown(%1)").arg(static_cast<int>(format));
    }
    return QString::fromUtf8(name);
}

QSize clampSize(const QSize &size) {
    return QSize(std::max(1, size.width()), std::max(1, size.height()));
}

double rationalToDouble(AVRational value) {
    if (value.num == 0 || value.den == 0) {
        return 0.0;
    }
    return static_cast<double>(value.num) / value.den;
}

std::chrono::nanoseconds frameDelayFor(
    const AVFrame *frame,
    AVRational timeBase,
    std::optional<int64_t> &lastPts,
    double fallbackSeconds
) {
    const int64_t pts = frame->best_effort_timestamp;
    double seconds = fallbackSeconds;

    if (pts != AV_NOPTS_VALUE && lastPts.has_value() && pts > *lastPts) {
        const double delta = static_cast<double>(pts - *lastPts) * rationalToDouble(timeBase);
        if (delta > 0.0 && delta < 1.0) {
            seconds = delta;
        }
    }

    if (pts != AV_NOPTS_VALUE) {
        lastPts = pts;
    }

    return std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::duration<double>(seconds)
    );
}

QStringList normalizeHwdecOrder(QStringList order) {
    QStringList normalized;
    normalized.reserve(order.size() + 1);

    for (QString &entry : order) {
        entry = entry.trimmed().toLower();
        if (entry.isEmpty()) {
            continue;
        }
        if (!normalized.contains(entry)) {
            normalized.push_back(entry);
        }
    }

    if (!normalized.contains(QStringLiteral("software"))) {
        normalized.push_back(QStringLiteral("software"));
    }

    return normalized;
}

QString readTrimmedFile(const QString &path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        return QString();
    }
    return QString::fromUtf8(file.readAll()).trimmed();
}

QString drmSysfsDevicePath(const QString &drmNodePath) {
    if (drmNodePath.isEmpty()) {
        return QString();
    }

    const QString nodeName = QFileInfo(drmNodePath).fileName();
    if (nodeName.isEmpty()) {
        return QString();
    }

    return QFileInfo(QStringLiteral("/sys/class/drm/%1/device").arg(nodeName)).canonicalFilePath();
}

QString drmNodeVendorId(const QString &drmNodePath) {
    const QString sysfsPath = drmSysfsDevicePath(drmNodePath);
    if (sysfsPath.isEmpty()) {
        return QString();
    }

    return readTrimmedFile(sysfsPath + QStringLiteral("/vendor")).toLower();
}

QString drmNodePciBusId(const QString &drmNodePath) {
    const QString sysfsPath = drmSysfsDevicePath(drmNodePath);
    if (sysfsPath.isEmpty()) {
        return QString();
    }

    const QString pciBusId = QFileInfo(sysfsPath).fileName();
    if (pciBusId.contains(QLatin1Char(':')) && pciBusId.contains(QLatin1Char('.'))) {
        return pciBusId;
    }

    return QString();
}

QString expandedCudaPciBusId(const QString &pciBusId) {
    const QStringList parts = pciBusId.split(QLatin1Char(':'));
    if (parts.size() != 3) {
        return QString();
    }

    return QStringLiteral("%1:%2:%3")
        .arg(parts[0].rightJustified(8, QLatin1Char('0')), parts[1], parts[2]);
}

using CUresult = int;
using CUdevice = int;
constexpr CUresult kCudaSuccess = 0;

struct CudaDriverApi {
    void *handle = nullptr;
    CUresult (*cuInit)(unsigned int) = nullptr;
    CUresult (*cuDeviceGetByPCIBusId)(CUdevice *, const char *) = nullptr;
    CUresult (*cuDeviceGetName)(char *, int, CUdevice) = nullptr;
    CUresult (*cuGetErrorName)(CUresult, const char **) = nullptr;
};

void unloadCudaDriverApi(CudaDriverApi *api) {
    if (api == nullptr || api->handle == nullptr) {
        return;
    }
    dlclose(api->handle);
    api->handle = nullptr;
}

QString cudaErrorString(const CudaDriverApi &api, CUresult code) {
    if (api.cuGetErrorName == nullptr) {
        return QStringLiteral("CUDA error %1").arg(code);
    }

    const char *name = nullptr;
    if (api.cuGetErrorName(code, &name) == kCudaSuccess && name != nullptr) {
        return QString::fromUtf8(name);
    }

    return QStringLiteral("CUDA error %1").arg(code);
}

std::optional<CudaDriverApi> loadCudaDriverApi(QString *error) {
    const char *libraryNames[] = {"libcuda.so.1", "libcuda.so"};
    void *handle = nullptr;
    for (const char *name : libraryNames) {
        handle = dlopen(name, RTLD_LAZY | RTLD_LOCAL);
        if (handle != nullptr) {
            break;
        }
    }

    if (handle == nullptr) {
        if (error != nullptr) {
            *error = QStringLiteral("dlopen(libcuda) failed");
        }
        return std::nullopt;
    }

    auto resolve = [handle](const char *primary, const char *fallback = nullptr) -> void * {
        void *symbol = dlsym(handle, primary);
        if (symbol == nullptr && fallback != nullptr) {
            symbol = dlsym(handle, fallback);
        }
        return symbol;
    };

    CudaDriverApi api{
        .handle = handle,
        .cuInit = reinterpret_cast<CUresult (*)(unsigned int)>(resolve("cuInit")),
        .cuDeviceGetByPCIBusId =
            reinterpret_cast<CUresult (*)(CUdevice *, const char *)>(
                resolve("cuDeviceGetByPCIBusId_v2", "cuDeviceGetByPCIBusId")
            ),
        .cuDeviceGetName =
            reinterpret_cast<CUresult (*)(char *, int, CUdevice)>(resolve("cuDeviceGetName")),
        .cuGetErrorName =
            reinterpret_cast<CUresult (*)(CUresult, const char **)>(resolve("cuGetErrorName")),
    };

    if (api.cuInit == nullptr || api.cuDeviceGetByPCIBusId == nullptr) {
        unloadCudaDriverApi(&api);
        if (error != nullptr) {
            *error = QStringLiteral("required CUDA driver symbols missing");
        }
        return std::nullopt;
    }

    return api;
}

struct HwDeviceSelection {
    bool skip = false;
    QString avDeviceString;
    QString label = QStringLiteral("<default-device>");
    QString reason;
};

HwDeviceSelection cudaDeviceSelectionForDrmNode(const QString &drmNodePath) {
    HwDeviceSelection selection{
        .skip = true,
        .reason = QStringLiteral("cuda exact device selection requires a preferred DRM render node"),
    };

    if (drmNodePath.isEmpty()) {
        selection.skip = false;
        selection.reason.clear();
        return selection;
    }

    const QString vendorId = drmNodeVendorId(drmNodePath);
    if (vendorId != QLatin1String("0x10de")) {
        selection.reason =
            QStringLiteral("preferred DRM node is not NVIDIA (%1)").arg(drmNodePath);
        return selection;
    }

    const QString pciBusId = drmNodePciBusId(drmNodePath);
    if (pciBusId.isEmpty()) {
        selection.reason =
            QStringLiteral("failed to resolve PCI bus id for %1").arg(drmNodePath);
        return selection;
    }

    QString loadError;
    auto api = loadCudaDriverApi(&loadError);
    if (!api.has_value()) {
        selection.reason =
            QStringLiteral("failed to load CUDA driver API (%1)").arg(loadError);
        return selection;
    }

    CUresult rc = api->cuInit(0);
    if (rc != kCudaSuccess) {
        selection.reason =
            QStringLiteral("cuInit failed (%1)").arg(cudaErrorString(*api, rc));
        unloadCudaDriverApi(&*api);
        return selection;
    }

    QStringList candidates{pciBusId};
    const QString expanded = expandedCudaPciBusId(pciBusId);
    if (!expanded.isEmpty() && expanded != pciBusId) {
        candidates.push_back(expanded);
    }

    CUdevice device = -1;
    QString resolvedBusId;
    for (const QString &candidate : candidates) {
        const QByteArray candidateBytes = candidate.toUtf8();
        rc = api->cuDeviceGetByPCIBusId(&device, candidateBytes.constData());
        if (rc == kCudaSuccess) {
            resolvedBusId = candidate;
            break;
        }
    }

    if (resolvedBusId.isEmpty()) {
        selection.reason =
            QStringLiteral("cuDeviceGetByPCIBusId failed for %1").arg(pciBusId);
        unloadCudaDriverApi(&*api);
        return selection;
    }

    selection.skip = false;
    selection.avDeviceString = QString::number(device);
    selection.label = QStringLiteral("cuda:%1 (%2 <- %3)")
        .arg(selection.avDeviceString, resolvedBusId, drmNodePath);

    if (api->cuDeviceGetName != nullptr) {
        char name[128] = {};
        if (api->cuDeviceGetName(name, static_cast<int>(sizeof(name)), device) == kCudaSuccess) {
            selection.label += QStringLiteral(" %1").arg(QString::fromUtf8(name));
        }
    }

    unloadCudaDriverApi(&*api);
    return selection;
}

AVHWDeviceType hwDeviceTypeForBackend(const QString &backend) {
    if (backend == QLatin1String("vaapi")) {
        return AV_HWDEVICE_TYPE_VAAPI;
    }
    if (backend == QLatin1String("cuda")) {
        return AV_HWDEVICE_TYPE_CUDA;
    }
    if (backend == QLatin1String("vulkan")) {
        return AV_HWDEVICE_TYPE_VULKAN;
    }
    if (backend == QLatin1String("qsv")) {
        return AV_HWDEVICE_TYPE_QSV;
    }
    return AV_HWDEVICE_TYPE_NONE;
}

HwDeviceSelection hwDeviceSelectionForBackend(const QString &backend, const QString &preferredDevicePath) {
    if (backend == QLatin1String("cuda")) {
        return cudaDeviceSelectionForDrmNode(preferredDevicePath);
    }

    if (preferredDevicePath.isEmpty()) {
        return {};
    }

    if (backend == QLatin1String("vaapi") || backend == QLatin1String("vulkan")) {
        return HwDeviceSelection{
            .skip = false,
            .avDeviceString = preferredDevicePath,
            .label = preferredDevicePath,
        };
    }

    return {};
}

const AVCodecHWConfig *codecHwConfigFor(const AVCodec *codec, AVHWDeviceType deviceType) {
    for (int i = 0;; ++i) {
        const AVCodecHWConfig *config = avcodec_get_hw_config(codec, i);
        if (config == nullptr) {
            return nullptr;
        }
        if (config->device_type != deviceType) {
            continue;
        }
        if ((config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX) == 0) {
            continue;
        }
        return config;
    }
}

struct DecoderHwState {
    AVPixelFormat hwPixelFormat = AV_PIX_FMT_NONE;
};

enum AVPixelFormat selectHwPixelFormat(AVCodecContext *ctx, const enum AVPixelFormat *pixFmts) {
    const auto *state = static_cast<const DecoderHwState *>(ctx->opaque);
    if (state == nullptr || state->hwPixelFormat == AV_PIX_FMT_NONE) {
        return pixFmts[0];
    }

    for (const enum AVPixelFormat *it = pixFmts; *it != AV_PIX_FMT_NONE; ++it) {
        if (*it == state->hwPixelFormat) {
            return *it;
        }
    }

    return pixFmts[0];
}

} // namespace

WallpaperVideo::WallpaperVideo(QObject *parent)
    : QObject(parent) {
    qInfo().noquote() << logPrefix() << "ffmpeg controller created";
}

WallpaperVideo::~WallpaperVideo() {
    stopDecoder();
}

QUrl WallpaperVideo::source() const {
    return m_source;
}

void WallpaperVideo::setSource(const QUrl &source) {
    if (m_source == source) {
        return;
    }

    m_source = source;
    emit sourceChanged();
    qInfo().noquote() << logPrefix() << "source set:"
                      << (m_source.isEmpty() ? QStringLiteral("<empty>") : m_source.toString());

    stopDecoder();
    clearFrame();
    setErrorString(QString());

    if (m_source.isEmpty()) {
        setStatus(QStringLiteral("idle"));
        setReady(false);
        return;
    }

    setStatus(QStringLiteral("loading"));
    setReady(false);
    restartDecoder();
}

bool WallpaperVideo::muted() const {
    return m_muted;
}

void WallpaperVideo::setMuted(bool muted) {
    if (m_muted == muted) {
        return;
    }

    m_muted = muted;
    emit mutedChanged();
}

bool WallpaperVideo::loopEnabled() const {
    return m_loopEnabled.load(std::memory_order_relaxed);
}

void WallpaperVideo::setLoopEnabled(bool loopEnabled) {
    if (m_loopEnabled.load(std::memory_order_relaxed) == loopEnabled) {
        return;
    }

    m_loopEnabled.store(loopEnabled, std::memory_order_relaxed);
    emit loopEnabledChanged();
}

qreal WallpaperVideo::volume() const {
    return m_volume;
}

void WallpaperVideo::setVolume(qreal volume) {
    const qreal clamped = std::clamp(volume, 0.0, 100.0);
    if (qFuzzyCompare(m_volume, clamped)) {
        return;
    }

    m_volume = clamped;
    emit volumeChanged();
}

QString WallpaperVideo::debugName() const {
    return m_debugName;
}

void WallpaperVideo::setDebugName(const QString &debugName) {
    if (m_debugName == debugName) {
        return;
    }

    m_debugName = debugName;
    emit debugNameChanged();
}

bool WallpaperVideo::isReady() const {
    return m_ready;
}

QString WallpaperVideo::status() const {
    return m_status;
}

QString WallpaperVideo::errorString() const {
    return m_errorString;
}

QString WallpaperVideo::hwdecCurrent() const {
    return m_hwdecCurrent;
}

QSize WallpaperVideo::videoSize() const {
    return m_videoSizeValue;
}

QSize WallpaperVideo::frameSize() const {
    return m_frameSizeValue;
}

QStringList WallpaperVideo::preferredHwdecOrder() const {
    return m_preferredHwdecOrder;
}

QString WallpaperVideo::preferredDevicePath() const {
    return m_preferredDevicePath;
}

WallpaperVideo::FrameSnapshot WallpaperVideo::frameSnapshot() const {
    QMutexLocker locker(&m_frameMutex);
    return FrameSnapshot{
        .image = m_frameImage,
        .size = m_frameSizeValue,
        .serial = m_frameSerial,
        .hasFrame = m_hasFrame,
    };
}

WallpaperVideo::HardwareFrameSnapshot WallpaperVideo::hardwareFrameSnapshot() const {
    QMutexLocker locker(&m_frameMutex);
    return HardwareFrameSnapshot{
        .frame = m_hardwareFrame,
        .size = m_videoSizeValue,
        .serial = m_frameSerial,
        .hasFrame = (m_hardwareFrame != nullptr),
    };
}

WallpaperVideo::StatsSnapshot WallpaperVideo::statsSnapshot() const {
    return StatsSnapshot{
        .status = m_status,
        .hwdecCurrent = m_hwdecCurrent,
        .videoSize = m_videoSizeValue,
        .frameSize = m_frameSizeValue,
        .decodedFrames = m_decodedFrames,
        .ready = m_ready,
    };
}

bool WallpaperVideo::hasRenderableFrame() const {
    QMutexLocker locker(&m_frameMutex);
    return m_hasFrame || static_cast<bool>(m_hardwareFrame);
}

void WallpaperVideo::ensureInitialized() {
    if (!m_source.isEmpty() && !m_decoderThread.joinable()) {
        restartDecoder();
    }
}

void WallpaperVideo::updateRenderTargetHint(QObject *item, const QSize &size) {
    if (item == nullptr) {
        return;
    }

    QMutexLocker locker(&m_hintMutex);
    const quintptr key = reinterpret_cast<quintptr>(item);
    const QSize clamped = size.isValid() ? clampSize(size) : QSize();

    if (clamped.isValid()) {
        auto hint = m_renderTargetHints.value(key);
        hint.size = clamped;
        m_renderTargetHints.insert(key, hint);
    } else {
        m_renderTargetHints.remove(key);
    }
}

void WallpaperVideo::removeRenderTargetHint(QObject *item) {
    if (item == nullptr) {
        return;
    }

    QMutexLocker locker(&m_hintMutex);
    m_renderTargetHints.remove(reinterpret_cast<quintptr>(item));
}

void WallpaperVideo::setCpuFrameRequired(QObject *item, bool required) {
    if (item == nullptr) {
        return;
    }

    QMutexLocker locker(&m_hintMutex);
    const quintptr key = reinterpret_cast<quintptr>(item);
    auto it = m_renderTargetHints.find(key);
    if (it == m_renderTargetHints.end()) {
        if (!required) {
            return;
        }
        m_renderTargetHints.insert(
            key,
            RenderTargetHint{
                .size = QSize(),
                .cpuFrameRequired = required,
            }
        );
        return;
    }

    if (it->cpuFrameRequired == required) {
        return;
    }
    it->cpuFrameRequired = required;
}

void WallpaperVideo::updateShareContextHint(QOpenGLContext *context) {
    Q_UNUSED(context)
}

void WallpaperVideo::setPreferredHwdecOrder(const QStringList &order) {
    const QStringList normalized = normalizeHwdecOrder(order);
    if (m_preferredHwdecOrder == normalized) {
        return;
    }

    m_preferredHwdecOrder = normalized;

    if (m_source.isEmpty()) {
        return;
    }

    stopDecoder();
    clearFrame();
    setErrorString(QString());
    setStatus(QStringLiteral("loading"));
    setReady(false);
    restartDecoder();
}

void WallpaperVideo::setPreferredDevicePath(const QString &path) {
    const QString normalized = path.trimmed();
    if (m_preferredDevicePath == normalized) {
        return;
    }

    m_preferredDevicePath = normalized;

    if (m_source.isEmpty()) {
        return;
    }

    stopDecoder();
    clearFrame();
    setErrorString(QString());
    setStatus(QStringLiteral("loading"));
    setReady(false);
    restartDecoder();
}

void WallpaperVideo::restartDecoder() {
    const QString localSource = m_source.isLocalFile() ? m_source.toLocalFile() : m_source.toString();
    if (localSource.isEmpty()) {
        setStatus(QStringLiteral("error"));
        setErrorString(QStringLiteral("invalid wallpaper video source"));
        return;
    }

    quint64 generation = 0;
    {
        std::lock_guard lock(m_threadMutex);
        m_stopRequested = false;
        m_decoderGeneration += 1;
        generation = m_decoderGeneration;
    }
    const QStringList hwdecOrder = m_preferredHwdecOrder;
    const QString preferredDevicePath = m_preferredDevicePath;
    m_decoderThread = std::thread([this, localSource, hwdecOrder, preferredDevicePath, generation]() {
        decoderMain(localSource, hwdecOrder, preferredDevicePath, generation);
    });
}

void WallpaperVideo::stopDecoder() {
    {
        std::lock_guard lock(m_threadMutex);
        m_stopRequested = true;
        m_decoderGeneration += 1;
    }
    m_stopCv.notify_all();

    if (m_decoderThread.joinable()) {
        m_decoderThread.join();
    }
}

void WallpaperVideo::decoderMain(
    QString localSource,
    QStringList hwdecOrder,
    QString preferredDevicePath,
    quint64 generation
) {
    AVFormatContext *formatContext = nullptr;
    int rc = avformat_open_input(&formatContext, localSource.toUtf8().constData(), nullptr, nullptr);
    if (rc < 0) {
        QMetaObject::invokeMethod(this, [this, rc, generation]() {
            if (shouldStop(generation)) {
                return;
            }
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("avformat_open_input failed: %1").arg(ffmpegErrorString(rc)));
        }, Qt::QueuedConnection);
        return;
    }

    auto formatGuard = std::unique_ptr<AVFormatContext, void (*)(AVFormatContext *)>(
        formatContext,
        [](AVFormatContext *ctx) {
            if (ctx != nullptr) {
                avformat_close_input(&ctx);
            }
        }
    );

    rc = avformat_find_stream_info(formatContext, nullptr);
    if (rc < 0) {
        QMetaObject::invokeMethod(this, [this, rc, generation]() {
            if (shouldStop(generation)) {
                return;
            }
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("avformat_find_stream_info failed: %1").arg(ffmpegErrorString(rc)));
        }, Qt::QueuedConnection);
        return;
    }

    const int streamIndex = av_find_best_stream(formatContext, AVMEDIA_TYPE_VIDEO, -1, -1, nullptr, 0);
    if (streamIndex < 0) {
        QMetaObject::invokeMethod(this, [this, generation]() {
            if (shouldStop(generation)) {
                return;
            }
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("no video stream found"));
        }, Qt::QueuedConnection);
        return;
    }

    AVStream *stream = formatContext->streams[streamIndex];
    const AVCodec *codec = avcodec_find_decoder(stream->codecpar->codec_id);
    if (codec == nullptr) {
        QMetaObject::invokeMethod(this, [this, generation]() {
            if (shouldStop(generation)) {
                return;
            }
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("no decoder found for wallpaper video"));
        }, Qt::QueuedConnection);
        return;
    }

    using CodecContextPtr = std::unique_ptr<AVCodecContext, void (*)(AVCodecContext *)>;
    auto makeCodecContext = [codec]() -> CodecContextPtr {
        return CodecContextPtr(
            avcodec_alloc_context3(codec),
            [](AVCodecContext *ctx) {
                if (ctx != nullptr) {
                    avcodec_free_context(&ctx);
                }
            }
        );
    };

    auto emitFatalError = [this, generation](const QString &message) {
        QMetaObject::invokeMethod(this, [this, generation, message]() {
            if (shouldStop(generation)) {
                return;
            }
            setStatus(QStringLiteral("error"));
            setErrorString(message);
        }, Qt::QueuedConnection);
    };

    auto prepareCodecContext = [&](AVCodecContext *ctx) -> bool {
        rc = avcodec_parameters_to_context(ctx, stream->codecpar);
        if (rc < 0) {
            emitFatalError(QStringLiteral("avcodec_parameters_to_context failed: %1").arg(ffmpegErrorString(rc)));
            return false;
        }
        return true;
    };

    DecoderHwState hwState;
    QString chosenHwdec = QStringLiteral("software");
    QString chosenHwdecDevice = QStringLiteral("<default-device>");
    CodecContextPtr codecContext(nullptr, [](AVCodecContext *ctx) {
        if (ctx != nullptr) {
            avcodec_free_context(&ctx);
        }
    });

    const QStringList normalizedHwdecOrder = normalizeHwdecOrder(hwdecOrder);
    for (const QString &backend : normalizedHwdecOrder) {
        if (backend == QLatin1String("software")) {
            continue;
        }

        const AVHWDeviceType deviceType = hwDeviceTypeForBackend(backend);
        if (deviceType == AV_HWDEVICE_TYPE_NONE) {
            qInfo().noquote() << logPrefix() << "skip unsupported hwdec backend" << backend;
            continue;
        }

        const AVCodecHWConfig *config = codecHwConfigFor(codec, deviceType);
        if (config == nullptr) {
            qInfo().noquote() << logPrefix() << "codec has no hw config for" << backend;
            continue;
        }

        const HwDeviceSelection deviceSelection =
            hwDeviceSelectionForBackend(backend, preferredDevicePath);
        if (deviceSelection.skip) {
            qInfo().noquote() << logPrefix() << "skip hwdec backend"
                              << backend << deviceSelection.reason;
            continue;
        }

        CodecContextPtr candidate = makeCodecContext();
        if (!candidate) {
            emitFatalError(QStringLiteral("avcodec_alloc_context3 failed"));
            return;
        }
        if (!prepareCodecContext(candidate.get())) {
            return;
        }

        AVBufferRef *deviceContext = nullptr;
        const QByteArray backendDevicePathBytes = deviceSelection.avDeviceString.toUtf8();
        rc = av_hwdevice_ctx_create(
            &deviceContext,
            deviceType,
            deviceSelection.avDeviceString.isEmpty() ? nullptr : backendDevicePathBytes.constData(),
            nullptr,
            0
        );
        if (rc < 0) {
            qInfo().noquote() << logPrefix() << "hwdec backend unavailable"
                              << backend
                              << deviceSelection.label
                              << ffmpegErrorString(rc);
            continue;
        }

        candidate->hw_device_ctx = av_buffer_ref(deviceContext);
        av_buffer_unref(&deviceContext);
        hwState.hwPixelFormat = config->pix_fmt;
        candidate->opaque = &hwState;
        candidate->get_format = &selectHwPixelFormat;

        rc = avcodec_open2(candidate.get(), codec, nullptr);
        if (rc < 0) {
            qInfo().noquote() << logPrefix() << "hwdec backend failed to open"
                              << backend << ffmpegErrorString(rc);
            continue;
        }

        chosenHwdec = backend;
        chosenHwdecDevice = deviceSelection.label;
        codecContext = std::move(candidate);
        break;
    }

    if (!codecContext) {
        codecContext = makeCodecContext();
        if (!codecContext) {
            emitFatalError(QStringLiteral("avcodec_alloc_context3 failed"));
            return;
        }
        if (!prepareCodecContext(codecContext.get())) {
            return;
        }

        rc = avcodec_open2(codecContext.get(), codec, nullptr);
        if (rc < 0) {
            emitFatalError(QStringLiteral("avcodec_open2 failed: %1").arg(ffmpegErrorString(rc)));
            return;
        }

        hwState.hwPixelFormat = AV_PIX_FMT_NONE;
    }

    const QSize videoSize(codecContext->width, codecContext->height);
    qInfo().noquote() << logPrefix() << "decoder opened"
                      << "hwdec =" << chosenHwdec
                      << "device =" << chosenHwdecDevice
                      << "hw_pix_fmt =" << pixelFormatName(hwState.hwPixelFormat);
    QMetaObject::invokeMethod(this, [this, videoSize, chosenHwdec, generation]() {
        if (shouldStop(generation)) {
            return;
        }
        if (m_videoSizeValue != videoSize) {
            m_videoSizeValue = videoSize;
            emit videoSizeChanged();
        }
        setHwdecCurrent(chosenHwdec);
    }, Qt::QueuedConnection);

    auto packet = std::unique_ptr<AVPacket, void (*)(AVPacket *)>(
        av_packet_alloc(),
        [](AVPacket *pkt) {
            if (pkt != nullptr) {
                av_packet_free(&pkt);
            }
        }
    );
    auto frame = std::unique_ptr<AVFrame, void (*)(AVFrame *)>(
        av_frame_alloc(),
        [](AVFrame *value) {
            if (value != nullptr) {
                av_frame_free(&value);
            }
        }
    );
    auto transferFrame = std::unique_ptr<AVFrame, void (*)(AVFrame *)>(
        av_frame_alloc(),
        [](AVFrame *value) {
            if (value != nullptr) {
                av_frame_free(&value);
            }
        }
    );

    if (!packet || !frame || !transferFrame) {
        QMetaObject::invokeMethod(this, [this, generation]() {
            if (shouldStop(generation)) {
                return;
            }
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("ffmpeg packet/frame allocation failed"));
        }, Qt::QueuedConnection);
        return;
    }

    SwsContext *swsContext = nullptr;
    QSize scaledSize;
    AVPixelFormat swsPixelFormat = AV_PIX_FMT_NONE;
    QSize swsSourceSize;
    std::optional<int64_t> lastPts;
    bool loggedHardwareFrame = false;

    const double fps = rationalToDouble(stream->avg_frame_rate);
    const double fallbackFrameSeconds = fps > 0.0 ? 1.0 / fps : 1.0 / 30.0;

    auto freeSws = [&]() {
        if (swsContext != nullptr) {
            sws_freeContext(swsContext);
            swsContext = nullptr;
        }
    };

    while (!shouldStop(generation)) {
        rc = av_read_frame(formatContext, packet.get());
        if (rc == AVERROR_EOF) {
            if (m_loopEnabled.load(std::memory_order_relaxed)) {
                av_seek_frame(formatContext, streamIndex, 0, AVSEEK_FLAG_BACKWARD);
                avcodec_flush_buffers(codecContext.get());
                lastPts.reset();
                continue;
            }

            QMetaObject::invokeMethod(this, [this, generation]() {
                if (shouldStop(generation)) {
                    return;
                }
                setStatus(QStringLiteral("ended"));
                setReady(true);
            }, Qt::QueuedConnection);
            break;
        }
        if (rc < 0) {
            QMetaObject::invokeMethod(this, [this, rc, generation]() {
                if (shouldStop(generation)) {
                    return;
                }
                setStatus(QStringLiteral("error"));
                setErrorString(QStringLiteral("av_read_frame failed: %1").arg(ffmpegErrorString(rc)));
            }, Qt::QueuedConnection);
            break;
        }

        if (packet->stream_index != streamIndex) {
            av_packet_unref(packet.get());
            continue;
        }

        rc = avcodec_send_packet(codecContext.get(), packet.get());
        av_packet_unref(packet.get());
        if (rc < 0) {
            QMetaObject::invokeMethod(this, [this, rc, generation]() {
                if (shouldStop(generation)) {
                    return;
                }
                setStatus(QStringLiteral("error"));
                setErrorString(QStringLiteral("avcodec_send_packet failed: %1").arg(ffmpegErrorString(rc)));
            }, Qt::QueuedConnection);
            break;
        }

        while (!shouldStop(generation)) {
            rc = avcodec_receive_frame(codecContext.get(), frame.get());
            if (rc == AVERROR(EAGAIN) || rc == AVERROR_EOF) {
                break;
            }
            if (rc < 0) {
                QMetaObject::invokeMethod(this, [this, rc, generation]() {
                    if (shouldStop(generation)) {
                        return;
                    }
                    setStatus(QStringLiteral("error"));
                    setErrorString(QStringLiteral("avcodec_receive_frame failed: %1").arg(ffmpegErrorString(rc)));
                }, Qt::QueuedConnection);
                freeSws();
                return;
            }

            AVFrame *convertFrame = frame.get();
            const bool needsCpuFrame = cpuFrameRequired();
            if (hwState.hwPixelFormat != AV_PIX_FMT_NONE &&
                frame->format == static_cast<int>(hwState.hwPixelFormat)) {
                if (!loggedHardwareFrame) {
                    QString swFormatName = QStringLiteral("none");
                    if (frame->hw_frames_ctx != nullptr) {
                        const auto *hwFrames =
                            reinterpret_cast<const AVHWFramesContext *>(frame->hw_frames_ctx->data);
                        if (hwFrames != nullptr) {
                            swFormatName = pixelFormatName(hwFrames->sw_format);
                        }
                    }
                    qInfo().noquote() << logPrefix() << "first hardware frame"
                                      << "format =" << pixelFormatName(static_cast<AVPixelFormat>(frame->format))
                                      << "sw_format =" << swFormatName
                                      << "size =" << frame->width << "x" << frame->height;
                    loggedHardwareFrame = true;
                }
                AVFrame *hardwareFrameRef = av_frame_clone(frame.get());
                if (hardwareFrameRef != nullptr) {
                    const AvFramePtr hardwareFrame(
                        hardwareFrameRef,
                        [](AVFrame *value) {
                            av_frame_free(&value);
                        }
                    );
                    QMetaObject::invokeMethod(this, [this, hardwareFrame, videoSize, generation, needsCpuFrame]() {
                        acceptHardwareFrame(
                            hardwareFrame,
                            videoSize,
                            generation,
                            true,
                            true
                        );
                    }, Qt::QueuedConnection);
                }

                if (!needsCpuFrame) {
                    const auto delay = frameDelayFor(frame.get(), stream->time_base, lastPts, fallbackFrameSeconds);
                    if (waitForStop(delay, generation)) {
                        freeSws();
                        return;
                    }
                    continue;
                }

                av_frame_unref(transferFrame.get());
                rc = av_hwframe_transfer_data(transferFrame.get(), frame.get(), 0);
                if (rc < 0) {
                    QMetaObject::invokeMethod(this, [this, rc, generation]() {
                        if (shouldStop(generation)) {
                            return;
                        }
                        setStatus(QStringLiteral("error"));
                        setErrorString(QStringLiteral("av_hwframe_transfer_data failed: %1").arg(ffmpegErrorString(rc)));
                    }, Qt::QueuedConnection);
                    freeSws();
                    return;
                }
                rc = av_frame_copy_props(transferFrame.get(), frame.get());
                if (rc < 0) {
                    QMetaObject::invokeMethod(this, [this, rc, generation]() {
                        if (shouldStop(generation)) {
                            return;
                        }
                        setStatus(QStringLiteral("error"));
                        setErrorString(QStringLiteral("av_frame_copy_props failed: %1").arg(ffmpegErrorString(rc)));
                    }, Qt::QueuedConnection);
                    freeSws();
                    return;
                }
                convertFrame = transferFrame.get();
            }

            const QSize nextScaledSize = targetFrameSize(videoSize);
            const QSize currentSourceSize(convertFrame->width, convertFrame->height);
            const AVPixelFormat currentPixelFormat = static_cast<AVPixelFormat>(convertFrame->format);
            if (swsContext == nullptr ||
                scaledSize != nextScaledSize ||
                swsPixelFormat != currentPixelFormat ||
                swsSourceSize != currentSourceSize) {
                freeSws();
                swsContext = sws_getContext(
                    convertFrame->width,
                    convertFrame->height,
                    currentPixelFormat,
                    nextScaledSize.width(),
                    nextScaledSize.height(),
                    AV_PIX_FMT_RGBA,
                    SWS_BILINEAR,
                    nullptr,
                    nullptr,
                    nullptr
                );
                scaledSize = nextScaledSize;
                swsPixelFormat = currentPixelFormat;
                swsSourceSize = currentSourceSize;
            }

            if (swsContext == nullptr) {
                QMetaObject::invokeMethod(this, [this, generation]() {
                    if (shouldStop(generation)) {
                        return;
                    }
                    setStatus(QStringLiteral("error"));
                    setErrorString(QStringLiteral("sws_getContext failed"));
                }, Qt::QueuedConnection);
                return;
            }

            QImage image(scaledSize, QImage::Format_RGBA8888);
            if (image.isNull()) {
                QMetaObject::invokeMethod(this, [this, generation]() {
                    if (shouldStop(generation)) {
                        return;
                    }
                    setStatus(QStringLiteral("error"));
                    setErrorString(QStringLiteral("failed to allocate decoded frame image"));
                }, Qt::QueuedConnection);
                freeSws();
                return;
            }

            uint8_t *dstData[4] = { image.bits(), nullptr, nullptr, nullptr };
            int dstLinesize[4] = { static_cast<int>(image.bytesPerLine()), 0, 0, 0 };

            sws_scale(
                swsContext,
                convertFrame->data,
                convertFrame->linesize,
                0,
                convertFrame->height,
                dstData,
                dstLinesize
            );

            const bool emitRenderableSignal = hwState.hwPixelFormat == AV_PIX_FMT_NONE;
            const bool countDecodedFrame = hwState.hwPixelFormat == AV_PIX_FMT_NONE;
            QMetaObject::invokeMethod(this, [this, image, videoSize, generation, countDecodedFrame, emitRenderableSignal]() {
                acceptFrame(
                    image,
                    videoSize,
                    generation,
                    countDecodedFrame,
                    emitRenderableSignal
                );
            }, Qt::QueuedConnection);

            const auto delay = frameDelayFor(frame.get(), stream->time_base, lastPts, fallbackFrameSeconds);
            if (waitForStop(delay, generation)) {
                freeSws();
                return;
            }
        }
    }

    freeSws();
}

void WallpaperVideo::acceptFrame(
    const QImage &image,
    const QSize &videoSize,
    quint64 generation,
    bool countDecodedFrame,
    bool emitRenderableSignal
) {
    if (shouldStop(generation)) {
        return;
    }

    const QSize frameSize = image.size();
    bool frameSizeChangedFlag = false;
    bool videoSizeChangedFlag = false;

    {
        QMutexLocker locker(&m_frameMutex);
        m_frameImage = image;
        m_frameSizeValue = frameSize;
        m_frameSerial += 1;
        if (countDecodedFrame) {
            m_decodedFrames += 1;
        }
        m_hasFrame = true;
        frameSizeChangedFlag = true;
    }

    if (m_videoSizeValue != videoSize) {
        m_videoSizeValue = videoSize;
        videoSizeChangedFlag = true;
    }

    if (!m_ready) {
        setReady(true);
    }
    if (m_status != QLatin1String("ready")) {
        setStatus(QStringLiteral("ready"));
    }

    if (videoSizeChangedFlag) {
        emit videoSizeChanged();
    }
    if (frameSizeChangedFlag) {
        emit frameSizeChanged();
    }
    if (emitRenderableSignal) {
        emit renderableFrameAvailable();
    }
    emit frameAvailable();
}

void WallpaperVideo::acceptHardwareFrame(
    const AvFramePtr &frame,
    const QSize &videoSize,
    quint64 generation,
    bool countDecodedFrame,
    bool emitRenderableSignal
) {
    if (shouldStop(generation) || !frame) {
        return;
    }

    bool videoSizeChangedFlag = false;
    {
        QMutexLocker locker(&m_frameMutex);
        m_hardwareFrame = frame;
        m_frameSerial += 1;
        if (countDecodedFrame) {
            m_decodedFrames += 1;
        }
    }

    if (m_videoSizeValue != videoSize) {
        m_videoSizeValue = videoSize;
        videoSizeChangedFlag = true;
    }

    if (!m_ready) {
        setReady(true);
    }
    if (m_status != QLatin1String("ready")) {
        setStatus(QStringLiteral("ready"));
    }

    if (videoSizeChangedFlag) {
        emit videoSizeChanged();
    }
    if (emitRenderableSignal) {
        emit renderableFrameAvailable();
    }
}

QSize WallpaperVideo::targetFrameSize(const QSize &videoSize) const {
    if (!videoSize.isValid()) {
        return QSize(1920, 1080);
    }

    QMutexLocker locker(&m_hintMutex);
    if (m_renderTargetHints.isEmpty()) {
        return videoSize;
    }

    const qreal videoAspect = static_cast<qreal>(videoSize.width()) / videoSize.height();
    int requiredWidth = videoSize.width();
    int requiredHeight = videoSize.height();

    for (auto it = m_renderTargetHints.cbegin(); it != m_renderTargetHints.cend(); ++it) {
        const QSize hint = it.value().size;
        if (!hint.isValid()) {
            continue;
        }

        const qreal screenAspect = static_cast<qreal>(hint.width()) / hint.height();
        if (screenAspect > videoAspect) {
            requiredWidth = std::max(requiredWidth, hint.width());
            requiredHeight = std::max(
                requiredHeight,
                static_cast<int>(std::ceil(hint.width() / videoAspect))
            );
        } else {
            requiredHeight = std::max(requiredHeight, hint.height());
            requiredWidth = std::max(
                requiredWidth,
                static_cast<int>(std::ceil(hint.height() * videoAspect))
            );
        }
    }

    return QSize(requiredWidth, requiredHeight);
}

bool WallpaperVideo::cpuFrameRequired() const {
    QMutexLocker locker(&m_hintMutex);
    if (m_renderTargetHints.isEmpty()) {
        return true;
    }

    for (auto it = m_renderTargetHints.cbegin(); it != m_renderTargetHints.cend(); ++it) {
        if (it.value().cpuFrameRequired) {
            return true;
        }
    }

    return false;
}

bool WallpaperVideo::shouldStop(quint64 generation) const {
    std::lock_guard lock(m_threadMutex);
    return m_stopRequested || m_decoderGeneration != generation;
}

bool WallpaperVideo::waitForStop(std::chrono::nanoseconds delay, quint64 generation) {
    std::unique_lock lock(m_threadMutex);
    return m_stopCv.wait_for(lock, delay, [this, generation]() {
        return m_stopRequested || m_decoderGeneration != generation;
    });
}

void WallpaperVideo::clearFrame() {
    bool frameSizeChangedFlag = false;
    bool videoSizeChangedFlag = false;

    {
        QMutexLocker locker(&m_frameMutex);
        m_frameImage = QImage();
        m_hardwareFrame.reset();
        frameSizeChangedFlag = m_frameSizeValue.isValid();
        m_frameSizeValue = QSize();
        m_hasFrame = false;
        m_frameSerial += 1;
    }

    videoSizeChangedFlag = m_videoSizeValue.isValid();
    m_videoSizeValue = QSize();

    if (frameSizeChangedFlag) {
        emit frameSizeChanged();
    }
    if (videoSizeChangedFlag) {
        emit videoSizeChanged();
    }
}

void WallpaperVideo::setReady(bool ready) {
    if (m_ready == ready) {
        return;
    }

    m_ready = ready;
    emit readyChanged();
}

void WallpaperVideo::setStatus(const QString &status) {
    if (m_status == status) {
        return;
    }

    m_status = status;
    qInfo().noquote() << logPrefix() << "status =" << m_status;
    emit statusChanged();
}

void WallpaperVideo::setErrorString(const QString &errorString) {
    if (m_errorString == errorString) {
        return;
    }

    m_errorString = errorString;
    if (!m_errorString.isEmpty()) {
        qWarning().noquote() << logPrefix() << "error =" << m_errorString;
    }
    emit errorStringChanged();
}

void WallpaperVideo::setHwdecCurrent(const QString &hwdecCurrent) {
    if (m_hwdecCurrent == hwdecCurrent) {
        return;
    }

    m_hwdecCurrent = hwdecCurrent;
    emit hwdecCurrentChanged();
}

QString WallpaperVideo::logPrefix() const {
    if (m_debugName.isEmpty()) {
        return QStringLiteral("[wallpaper-ffmpeg]");
    }
    return QStringLiteral("[wallpaper-ffmpeg %1]").arg(m_debugName);
}
