// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperVideo.hpp"

#include <memory>

#include <QDebug>
#include <QMetaObject>

#include "WallpaperVideoInternal.hpp"

extern "C" {
#include <libavformat/avformat.h>
#include <libavutil/imgutils.h>
#include <libavutil/pixdesc.h>
#include <libswscale/swscale.h>
}

using quicksov::wallpaper_ffmpeg::detail::codecHwConfigFor;
using quicksov::wallpaper_ffmpeg::detail::DecoderHwState;
using quicksov::wallpaper_ffmpeg::detail::ffmpegErrorString;
using quicksov::wallpaper_ffmpeg::detail::frameDelayFor;
using quicksov::wallpaper_ffmpeg::detail::hwDeviceSelectionForBackend;
using quicksov::wallpaper_ffmpeg::detail::hwDeviceTypeForBackend;
using quicksov::wallpaper_ffmpeg::detail::normalizeHwdecOrder;
using quicksov::wallpaper_ffmpeg::detail::pixelFormatName;
using quicksov::wallpaper_ffmpeg::detail::rationalToDouble;
using quicksov::wallpaper_ffmpeg::detail::selectHwPixelFormat;

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

        const auto deviceSelection =
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
