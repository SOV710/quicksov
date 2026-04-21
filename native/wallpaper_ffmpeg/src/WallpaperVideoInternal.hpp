// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <chrono>
#include <optional>

#include <QSize>
#include <QString>
#include <QStringList>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
}

namespace quicksov::wallpaper_ffmpeg::detail {

QString ffmpegErrorString(int code);
QString pixelFormatName(AVPixelFormat format);
QSize clampSize(const QSize &size);
double rationalToDouble(AVRational value);
std::chrono::nanoseconds frameDelayFor(
    const AVFrame *frame,
    AVRational timeBase,
    std::optional<int64_t> &lastPts,
    double fallbackSeconds
);
QStringList normalizeHwdecOrder(QStringList order);

struct HwDeviceSelection {
    bool skip = false;
    QString avDeviceString;
    QString label = QStringLiteral("<default-device>");
    QString reason;
};

AVHWDeviceType hwDeviceTypeForBackend(const QString &backend);
HwDeviceSelection hwDeviceSelectionForBackend(
    const QString &backend,
    const QString &preferredDevicePath
);
const AVCodecHWConfig *codecHwConfigFor(
    const AVCodec *codec,
    AVHWDeviceType deviceType
);

struct DecoderHwState {
    AVPixelFormat hwPixelFormat = AV_PIX_FMT_NONE;
};

enum AVPixelFormat selectHwPixelFormat(
    AVCodecContext *ctx,
    const enum AVPixelFormat *pixFmts
);

} // namespace quicksov::wallpaper_ffmpeg::detail
