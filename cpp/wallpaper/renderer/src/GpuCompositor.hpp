// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "SnapshotModel.hpp"

#include "../../decoder/ffmpeg/VideoDecoder.hpp"

#include <map>

extern "C" {
#include <EGL/egl.h>
#include <gbm.h>
}

#define PL_LIBAV_IMPLEMENTATION 0
#include <libplacebo/colorspace.h>
#include <libplacebo/common.h>
#include <libplacebo/gpu.h>
#include <libplacebo/log.h>
#include <libplacebo/opengl.h>
#include <libplacebo/renderer.h>
#include <libplacebo/utils/libav.h>

namespace quicksov::wallpaper::renderer {

using VideoDecoder = quicksov::wallpaper::decoder::ffmpeg::VideoDecoder;

class GpuCompositor final {
public:
    ~GpuCompositor();

    bool initialize(gbm_device *gbmDevice, QString *error);
    void destroy();
    [[nodiscard]] bool available() const;
    void releaseTarget(quintptr key);
    bool renderToDmabuf(
        const VideoDecoder::HardwareFrameSnapshot &source,
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
        VideoDecoder::AvFramePtr frame;
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
        const VideoDecoder::HardwareFrameSnapshot &source,
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

} // namespace quicksov::wallpaper::renderer
