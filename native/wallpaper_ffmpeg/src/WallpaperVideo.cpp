// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperVideo.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <memory>

#include <QDebug>
#include <QMetaObject>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/imgutils.h>
#include <libswscale/swscale.h>
}

namespace {

QString ffmpegErrorString(int code) {
    char buffer[AV_ERROR_MAX_STRING_SIZE] = {};
    av_strerror(code, buffer, sizeof(buffer));
    return QString::fromUtf8(buffer);
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
    return m_loopEnabled;
}

void WallpaperVideo::setLoopEnabled(bool loopEnabled) {
    if (m_loopEnabled == loopEnabled) {
        return;
    }

    m_loopEnabled = loopEnabled;
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

WallpaperVideo::FrameSnapshot WallpaperVideo::frameSnapshot() const {
    QMutexLocker locker(&m_frameMutex);
    return FrameSnapshot{
        .image = m_frameImage,
        .size = m_frameSizeValue,
        .serial = m_frameSerial,
        .hasFrame = m_hasFrame,
    };
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
        m_renderTargetHints.insert(key, clamped);
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

void WallpaperVideo::updateShareContextHint(QOpenGLContext *context) {
    Q_UNUSED(context)
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
    m_decoderThread = std::thread([this, localSource, generation]() {
        decoderMain(localSource, generation);
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

void WallpaperVideo::decoderMain(QString localSource, quint64 generation) {
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

    auto codecContext = std::unique_ptr<AVCodecContext, void (*)(AVCodecContext *)>(
        avcodec_alloc_context3(codec),
        [](AVCodecContext *ctx) {
            if (ctx != nullptr) {
                avcodec_free_context(&ctx);
            }
        }
    );
    if (!codecContext) {
        QMetaObject::invokeMethod(this, [this, generation]() {
            if (shouldStop(generation)) {
                return;
            }
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("avcodec_alloc_context3 failed"));
        }, Qt::QueuedConnection);
        return;
    }

    rc = avcodec_parameters_to_context(codecContext.get(), stream->codecpar);
    if (rc < 0) {
        QMetaObject::invokeMethod(this, [this, rc, generation]() {
            if (shouldStop(generation)) {
                return;
            }
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("avcodec_parameters_to_context failed: %1").arg(ffmpegErrorString(rc)));
        }, Qt::QueuedConnection);
        return;
    }

    rc = avcodec_open2(codecContext.get(), codec, nullptr);
    if (rc < 0) {
        QMetaObject::invokeMethod(this, [this, rc, generation]() {
            if (shouldStop(generation)) {
                return;
            }
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("avcodec_open2 failed: %1").arg(ffmpegErrorString(rc)));
        }, Qt::QueuedConnection);
        return;
    }

    const QSize videoSize(codecContext->width, codecContext->height);
    QMetaObject::invokeMethod(this, [this, videoSize, generation]() {
        if (shouldStop(generation)) {
            return;
        }
        if (m_videoSizeValue != videoSize) {
            m_videoSizeValue = videoSize;
            emit videoSizeChanged();
        }
        setHwdecCurrent(QStringLiteral("software"));
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

    if (!packet || !frame) {
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
    std::optional<int64_t> lastPts;

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
            if (m_loopEnabled) {
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

            const QSize nextScaledSize = targetFrameSize(videoSize);
            if (swsContext == nullptr || scaledSize != nextScaledSize) {
                freeSws();
                swsContext = sws_getContext(
                    frame->width,
                    frame->height,
                    static_cast<AVPixelFormat>(frame->format),
                    nextScaledSize.width(),
                    nextScaledSize.height(),
                    AV_PIX_FMT_RGBA,
                    SWS_BILINEAR,
                    nullptr,
                    nullptr,
                    nullptr
                );
                scaledSize = nextScaledSize;
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
                frame->data,
                frame->linesize,
                0,
                frame->height,
                dstData,
                dstLinesize
            );

            QMetaObject::invokeMethod(this, [this, image, videoSize, generation]() {
                acceptFrame(image, videoSize, generation);
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

void WallpaperVideo::acceptFrame(const QImage &image, const QSize &videoSize, quint64 generation) {
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
    emit frameAvailable();
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
        const QSize hint = it.value();
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
