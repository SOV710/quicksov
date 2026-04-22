// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "VideoDecoderInternal.hpp"
#include "WallpaperContract.hpp"

#include <algorithm>
#include <dlfcn.h>

#include <QFile>
#include <QFileInfo>

extern "C" {
#include <libavutil/error.h>
#include <libavutil/pixdesc.h>
}

namespace quicksov::wallpaper::decoder::ffmpeg::detail {

namespace {

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

} // namespace

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
    QStringList normalized = shared::normalizeDecodeBackendOrder(std::move(order));
    if (normalized.isEmpty()) {
        normalized = shared::defaultDecodeBackendOrder();
    }
    return normalized;
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

HwDeviceSelection hwDeviceSelectionForBackend(
    const QString &backend,
    const QString &preferredDevicePath
) {
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

enum AVPixelFormat selectHwPixelFormat(
    AVCodecContext *ctx,
    const enum AVPixelFormat *pixFmts
) {
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

} // namespace quicksov::wallpaper::decoder::ffmpeg::detail
