// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperVideo.hpp"

#include <algorithm>
#include <cmath>

#include <QDebug>

#include "WallpaperVideoInternal.hpp"

using quicksov::wallpaper_ffmpeg::detail::clampSize;
using quicksov::wallpaper_ffmpeg::detail::normalizeHwdecOrder;

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
