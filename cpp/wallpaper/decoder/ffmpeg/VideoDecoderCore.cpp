// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "VideoDecoder.hpp"

#include <algorithm>
#include <cmath>

#include <QDebug>

#include "VideoDecoderInternal.hpp"

namespace quicksov::wallpaper::decoder::ffmpeg {

using detail::clampSize;
using detail::normalizeHwdecOrder;

VideoDecoder::VideoDecoder(QObject *parent)
    : QObject(parent) {
    qInfo().noquote() << logPrefix() << "ffmpeg controller created";
}

VideoDecoder::~VideoDecoder() {
    stopDecoder();
}

QUrl VideoDecoder::source() const {
    return m_source;
}

void VideoDecoder::setSource(const QUrl &source) {
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

bool VideoDecoder::muted() const {
    return m_muted;
}

void VideoDecoder::setMuted(bool muted) {
    if (m_muted == muted) {
        return;
    }

    m_muted = muted;
    emit mutedChanged();
}

bool VideoDecoder::loopEnabled() const {
    return m_loopEnabled.load(std::memory_order_relaxed);
}

void VideoDecoder::setLoopEnabled(bool loopEnabled) {
    if (m_loopEnabled.load(std::memory_order_relaxed) == loopEnabled) {
        return;
    }

    m_loopEnabled.store(loopEnabled, std::memory_order_relaxed);
    emit loopEnabledChanged();
}

qreal VideoDecoder::volume() const {
    return m_volume;
}

void VideoDecoder::setVolume(qreal volume) {
    const qreal clamped = std::clamp(volume, 0.0, 100.0);
    if (qFuzzyCompare(m_volume, clamped)) {
        return;
    }

    m_volume = clamped;
    emit volumeChanged();
}

QString VideoDecoder::debugName() const {
    return m_debugName;
}

void VideoDecoder::setDebugName(const QString &debugName) {
    if (m_debugName == debugName) {
        return;
    }

    m_debugName = debugName;
    emit debugNameChanged();
}

bool VideoDecoder::isReady() const {
    return m_ready;
}

QString VideoDecoder::status() const {
    return m_status;
}

QString VideoDecoder::errorString() const {
    return m_errorString;
}

QString VideoDecoder::hwdecCurrent() const {
    return m_hwdecCurrent;
}

QSize VideoDecoder::videoSize() const {
    return m_videoSizeValue;
}

QSize VideoDecoder::frameSize() const {
    return m_frameSizeValue;
}

QStringList VideoDecoder::preferredHwdecOrder() const {
    return m_preferredHwdecOrder;
}

QString VideoDecoder::preferredDevicePath() const {
    return m_preferredDevicePath;
}

VideoDecoder::FrameSnapshot VideoDecoder::frameSnapshot() const {
    QMutexLocker locker(&m_frameMutex);
    return FrameSnapshot{
        .image = m_frameImage,
        .size = m_frameSizeValue,
        .serial = m_frameSerial,
        .hasFrame = m_hasFrame,
    };
}

VideoDecoder::HardwareFrameSnapshot VideoDecoder::hardwareFrameSnapshot() const {
    QMutexLocker locker(&m_frameMutex);
    return HardwareFrameSnapshot{
        .frame = m_hardwareFrame,
        .size = m_videoSizeValue,
        .serial = m_frameSerial,
        .hasFrame = (m_hardwareFrame != nullptr),
    };
}

VideoDecoder::StatsSnapshot VideoDecoder::statsSnapshot() const {
    return StatsSnapshot{
        .status = m_status,
        .hwdecCurrent = m_hwdecCurrent,
        .videoSize = m_videoSizeValue,
        .frameSize = m_frameSizeValue,
        .decodedFrames = m_decodedFrames,
        .ready = m_ready,
    };
}

bool VideoDecoder::hasRenderableFrame() const {
    QMutexLocker locker(&m_frameMutex);
    return m_hasFrame || static_cast<bool>(m_hardwareFrame);
}

void VideoDecoder::ensureInitialized() {
    if (!m_source.isEmpty() && !m_decoderThread.joinable()) {
        restartDecoder();
    }
}

void VideoDecoder::updateRenderTargetHint(QObject *item, const QSize &size) {
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

void VideoDecoder::removeRenderTargetHint(QObject *item) {
    if (item == nullptr) {
        return;
    }

    QMutexLocker locker(&m_hintMutex);
    m_renderTargetHints.remove(reinterpret_cast<quintptr>(item));
}

void VideoDecoder::setCpuFrameRequired(QObject *item, bool required) {
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

void VideoDecoder::updateShareContextHint(QOpenGLContext *context) {
    Q_UNUSED(context)
}

void VideoDecoder::setPreferredHwdecOrder(const QStringList &order) {
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

void VideoDecoder::setPreferredDevicePath(const QString &path) {
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

void VideoDecoder::acceptFrame(
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

void VideoDecoder::acceptHardwareFrame(
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

QSize VideoDecoder::targetFrameSize(const QSize &videoSize) const {
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

bool VideoDecoder::cpuFrameRequired() const {
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

bool VideoDecoder::shouldStop(quint64 generation) const {
    std::lock_guard lock(m_threadMutex);
    return m_stopRequested || m_decoderGeneration != generation;
}

bool VideoDecoder::waitForStop(std::chrono::nanoseconds delay, quint64 generation) {
    std::unique_lock lock(m_threadMutex);
    return m_stopCv.wait_for(lock, delay, [this, generation]() {
        return m_stopRequested || m_decoderGeneration != generation;
    });
}

void VideoDecoder::clearFrame() {
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

void VideoDecoder::setReady(bool ready) {
    if (m_ready == ready) {
        return;
    }

    m_ready = ready;
    emit readyChanged();
}

void VideoDecoder::setStatus(const QString &status) {
    if (m_status == status) {
        return;
    }

    m_status = status;
    qInfo().noquote() << logPrefix() << "status =" << m_status;
    emit statusChanged();
}

void VideoDecoder::setErrorString(const QString &errorString) {
    if (m_errorString == errorString) {
        return;
    }

    m_errorString = errorString;
    if (!m_errorString.isEmpty()) {
        qWarning().noquote() << logPrefix() << "error =" << m_errorString;
    }
    emit errorStringChanged();
}

void VideoDecoder::setHwdecCurrent(const QString &hwdecCurrent) {
    if (m_hwdecCurrent == hwdecCurrent) {
        return;
    }

    m_hwdecCurrent = hwdecCurrent;
    emit hwdecCurrentChanged();
}

QString VideoDecoder::logPrefix() const {
    if (m_debugName.isEmpty()) {
        return QStringLiteral("[wallpaper-ffmpeg]");
    }
    return QStringLiteral("[wallpaper-ffmpeg %1]").arg(m_debugName);
}

} // namespace quicksov::wallpaper::decoder::ffmpeg
