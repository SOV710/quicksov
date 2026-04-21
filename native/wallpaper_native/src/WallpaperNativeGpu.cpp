// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperNativeRuntime.hpp"

#include <QDebug>

extern "C" {
#include <GLES2/gl2.h>
}

namespace quicksov::wallpaper_native {

GpuCompositor::~GpuCompositor() {
    destroy();
}

bool GpuCompositor::initialize(gbm_device *gbmDevice, QString *error) {
    if (m_renderer != nullptr) {
        return true;
    }

    EGLDisplay display = EGL_NO_DISPLAY;
    const char *backendName = "default";
#ifdef EGL_PLATFORM_GBM_KHR
    if (gbmDevice != nullptr) {
        display = eglGetPlatformDisplay(EGL_PLATFORM_GBM_KHR, gbmDevice, nullptr);
        backendName = "gbm";
    }
#endif
    if (display == EGL_NO_DISPLAY && gbmDevice != nullptr) {
        display = eglGetDisplay(reinterpret_cast<EGLNativeDisplayType>(gbmDevice));
        backendName = "gbm-legacy";
    }
#ifdef EGL_PLATFORM_SURFACELESS_MESA
    if (display == EGL_NO_DISPLAY) {
        display = eglGetPlatformDisplay(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, nullptr);
        backendName = "surfaceless";
    }
#endif
    if (display == EGL_NO_DISPLAY) {
        display = eglGetDisplay(EGL_DEFAULT_DISPLAY);
        backendName = "default";
    }
    if (display == EGL_NO_DISPLAY) {
        if (error != nullptr) {
            *error = QStringLiteral("eglGetDisplay failed");
        }
        return false;
    }

    EGLint major = 0;
    EGLint minor = 0;
    if (!eglInitialize(display, &major, &minor)) {
        if (error != nullptr) {
            *error = QStringLiteral("eglInitialize failed");
        }
        return false;
    }

    if (!eglBindAPI(EGL_OPENGL_ES_API)) {
        if (error != nullptr) {
            *error = QStringLiteral("eglBindAPI(EGL_OPENGL_ES_API) failed");
        }
        eglTerminate(display);
        return false;
    }

    const EGLint configAttrs[] = {
        EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT,
        EGL_RED_SIZE, 8,
        EGL_GREEN_SIZE, 8,
        EGL_BLUE_SIZE, 8,
        EGL_ALPHA_SIZE, 8,
        EGL_NONE,
    };
    EGLConfig config = nullptr;
    EGLint numConfigs = 0;
    if (!eglChooseConfig(display, configAttrs, &config, 1, &numConfigs) || numConfigs <= 0) {
        if (error != nullptr) {
            *error = QStringLiteral("eglChooseConfig failed");
        }
        eglTerminate(display);
        return false;
    }

    const EGLint surfaceAttrs[] = {
        EGL_WIDTH, 1,
        EGL_HEIGHT, 1,
        EGL_NONE,
    };
    EGLSurface surface = eglCreatePbufferSurface(display, config, surfaceAttrs);

    const EGLint contextAttrs[] = {
        EGL_CONTEXT_CLIENT_VERSION, 2,
        EGL_NONE,
    };
    EGLContext context = eglCreateContext(display, config, EGL_NO_CONTEXT, contextAttrs);
    if (context == EGL_NO_CONTEXT) {
        if (error != nullptr) {
            *error = QStringLiteral("eglCreateContext failed");
        }
        if (surface != EGL_NO_SURFACE) {
            eglDestroySurface(display, surface);
        }
        eglTerminate(display);
        return false;
    }

    const EGLSurface drawSurface = (surface != EGL_NO_SURFACE) ? surface : EGL_NO_SURFACE;
    if (!eglMakeCurrent(display, drawSurface, drawSurface, context)) {
        if (error != nullptr) {
            *error = QStringLiteral("eglMakeCurrent failed");
        }
        eglDestroyContext(display, context);
        if (surface != EGL_NO_SURFACE) {
            eglDestroySurface(display, surface);
        }
        eglTerminate(display);
        return false;
    }

    struct pl_log_params logParams = pl_log_default_params;
    logParams.log_cb = &GpuCompositor::logCallback;
    logParams.log_level = PL_LOG_WARN;
    m_log = pl_log_create(PL_API_VER, &logParams);

    struct pl_opengl_params openglParams = pl_opengl_default_params;
    openglParams.allow_software = false;
    openglParams.get_proc_addr = reinterpret_cast<pl_voidfunc_t (*)(const char *)>(eglGetProcAddress);
    openglParams.egl_display = display;
    openglParams.egl_context = context;
    m_opengl = pl_opengl_create(m_log, &openglParams);
    if (m_opengl == nullptr) {
        if (error != nullptr) {
            *error = QStringLiteral("pl_opengl_create failed");
        }
        cleanupEgl(display, surface, context);
        pl_log_destroy(&m_log);
        return false;
    }

    m_renderer = pl_renderer_create(m_log, m_opengl->gpu);
    if (m_renderer == nullptr) {
        if (error != nullptr) {
            *error = QStringLiteral("pl_renderer_create failed");
        }
        pl_opengl_destroy(&m_opengl);
        cleanupEgl(display, surface, context);
        pl_log_destroy(&m_log);
        return false;
    }

    if ((m_opengl->gpu->import_caps.tex & PL_HANDLE_DMA_BUF) == 0 ||
        !pl_test_pixfmt(m_opengl->gpu, AV_PIX_FMT_DRM_PRIME)) {
        if (error != nullptr) {
            *error = QStringLiteral("libplacebo GPU does not support DRM_PRIME import");
        }
        pl_renderer_destroy(&m_renderer);
        pl_opengl_destroy(&m_opengl);
        cleanupEgl(display, surface, context);
        pl_log_destroy(&m_log);
        return false;
    }

    m_display = display;
    m_surface = surface;
    m_context = context;
    qInfo().noquote() << kLogPrefix << "gpu compositor initialized"
                      << "backend =" << backendName
                      << "egl =" << QStringLiteral("%1.%2").arg(major).arg(minor)
                      << "dmabuf_import =" << static_cast<bool>(m_opengl->gpu->import_caps.tex & PL_HANDLE_DMA_BUF);
    return true;
}

void GpuCompositor::destroy() {
    if (m_display == EGL_NO_DISPLAY) {
        return;
    }

    makeCurrent();
    releaseSourceFrame();
    for (auto &entry : m_targets) {
        if (entry.second.texture != nullptr) {
            pl_tex_destroy(m_opengl->gpu, &entry.second.texture);
        }
        if (entry.second.fd >= 0) {
            ::close(entry.second.fd);
        }
    }
    m_targets.clear();

    if (m_renderer != nullptr) {
        pl_renderer_destroy(&m_renderer);
    }
    if (m_opengl != nullptr) {
        pl_opengl_destroy(&m_opengl);
    }
    pl_log_destroy(&m_log);

    cleanupEgl(m_display, m_surface, m_context);
    m_display = EGL_NO_DISPLAY;
    m_surface = EGL_NO_SURFACE;
    m_context = EGL_NO_CONTEXT;
}

bool GpuCompositor::available() const {
    return m_renderer != nullptr && m_opengl != nullptr;
}

void GpuCompositor::releaseTarget(quintptr key) {
    auto it = m_targets.find(key);
    if (it == m_targets.end()) {
        return;
    }

    makeCurrent();
    if (it->second.texture != nullptr) {
        pl_tex_destroy(m_opengl->gpu, &it->second.texture);
    }
    if (it->second.fd >= 0) {
        ::close(it->second.fd);
    }
    m_targets.erase(it);
}

bool GpuCompositor::renderToDmabuf(
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
) {
    if (!available() || !source.hasFrame || !source.frame || targetFd < 0) {
        return false;
    }

    if (!makeCurrent()) {
        if (error != nullptr) {
            *error = QStringLiteral("eglMakeCurrent failed");
        }
        return false;
    }

    TargetTexture *target = ensureTarget(
        targetKey,
        targetFd,
        width,
        height,
        stride,
        offset,
        drmFormat,
        modifier,
        error
    );
    if (target == nullptr || target->texture == nullptr) {
        return false;
    }

    const SourceFrame *mappedSource = ensureSourceFrame(source, error);
    if (mappedSource == nullptr) {
        return false;
    }

    struct pl_frame image = mappedSource->image;
    const QRectF sourceRect = coverSourceRect(cropRectFor(source.size, crop), targetSize);
    image.crop = pl_rect2df{
        static_cast<float>(sourceRect.left()),
        static_cast<float>(sourceRect.top()),
        static_cast<float>(sourceRect.right()),
        static_cast<float>(sourceRect.bottom()),
    };

    struct pl_frame targetFrame = {};
    targetFrame.num_planes = 1;
    targetFrame.planes[0].texture = target->texture;
    targetFrame.planes[0].components = 4;
    targetFrame.planes[0].component_mapping[0] = 0;
    targetFrame.planes[0].component_mapping[1] = 1;
    targetFrame.planes[0].component_mapping[2] = 2;
    targetFrame.planes[0].component_mapping[3] = 3;
    targetFrame.repr = pl_color_repr_rgb;
    targetFrame.color = pl_color_space_unknown;
    targetFrame.crop = pl_rect2df{0.0f, 0.0f, static_cast<float>(width), static_cast<float>(height)};

    const bool ok = pl_render_image(m_renderer, &image, &targetFrame, &pl_render_fast_params);
    if (conservativeSync) {
        glFinish();
    } else {
        glFlush();
    }

    if (!ok) {
        if (error != nullptr) {
            *error = QStringLiteral("pl_render_image failed");
        }
        return false;
    }

    return true;
}

void GpuCompositor::logCallback(void *, enum pl_log_level level, const char *msg) {
    const QString text = QString::fromUtf8(msg);
    switch (level) {
    case PL_LOG_FATAL:
    case PL_LOG_ERR:
    case PL_LOG_WARN:
        qWarning().noquote() << kLogPrefix << "[libplacebo]" << text;
        break;
    default:
        qDebug().noquote() << kLogPrefix << "[libplacebo]" << text;
        break;
    }
}

void GpuCompositor::cleanupEgl(EGLDisplay display, EGLSurface surface, EGLContext context) {
    if (display == EGL_NO_DISPLAY) {
        return;
    }
    eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
    if (context != EGL_NO_CONTEXT) {
        eglDestroyContext(display, context);
    }
    if (surface != EGL_NO_SURFACE) {
        eglDestroySurface(display, surface);
    }
    eglTerminate(display);
}

bool GpuCompositor::makeCurrent() const {
    return m_display != EGL_NO_DISPLAY &&
           eglMakeCurrent(m_display, m_surface, m_surface, m_context);
}

void GpuCompositor::releaseSourceFrame() {
    if (m_sourceFrame.frame != nullptr) {
        pl_unmap_avframe(m_opengl->gpu, &m_sourceFrame.image);
    }
    m_sourceFrame = SourceFrame{};
}

GpuCompositor::SourceFrame *GpuCompositor::ensureSourceFrame(
    const WallpaperVideo::HardwareFrameSnapshot &source,
    QString *error
) {
    if (m_sourceFrame.frame == source.frame &&
        m_sourceFrame.serial == source.serial &&
        m_sourceFrame.frame != nullptr) {
        return &m_sourceFrame;
    }

    releaseSourceFrame();

    if (!pl_map_avframe(m_opengl->gpu, &m_sourceFrame.image, m_sourceFrame.textures, source.frame.get())) {
        if (error != nullptr) {
            *error = QStringLiteral("pl_map_avframe failed (%1 %2)")
                         .arg(describeAvFrame(source.frame.get()), describeDerivedDrmFrame(source.frame.get()));
        }
        m_sourceFrame = SourceFrame{};
        return nullptr;
    }

    m_sourceFrame.serial = source.serial;
    m_sourceFrame.frame = source.frame;
    return &m_sourceFrame;
}

GpuCompositor::TargetTexture *GpuCompositor::ensureTarget(
    quintptr key,
    int targetFd,
    int width,
    int height,
    int stride,
    int offset,
    uint32_t drmFormat,
    uint64_t modifier,
    QString *error
) {
    auto it = m_targets.find(key);
    if (it != m_targets.end() &&
        it->second.width == width &&
        it->second.height == height &&
        it->second.stride == stride &&
        it->second.offset == offset &&
        it->second.drmFormat == drmFormat &&
        it->second.modifier == modifier) {
        return &it->second;
    }

    if (it != m_targets.end()) {
        releaseTarget(key);
    }

    const int dupFd = ::dup(targetFd);
    if (dupFd < 0) {
        if (error != nullptr) {
            *error = QStringLiteral("dup(targetFd) failed");
        }
        return nullptr;
    }

    const pl_fmt format = pl_find_fourcc(m_opengl->gpu, drmFormat);
    if (format == nullptr) {
        ::close(dupFd);
        if (error != nullptr) {
            *error = QStringLiteral("pl_find_fourcc failed");
        }
        return nullptr;
    }

    TargetTexture target{
        .fd = dupFd,
        .width = width,
        .height = height,
        .stride = stride,
        .offset = offset,
        .drmFormat = drmFormat,
        .modifier = modifier,
    };
    struct pl_tex_params params = {};
    params.w = width;
    params.h = height;
    params.format = format;
    params.renderable = true;
    params.blit_dst = static_cast<bool>(format->caps & PL_FMT_CAP_BLITTABLE);
    params.import_handle = PL_HANDLE_DMA_BUF;
    params.shared_mem.handle.fd = dupFd;
    params.shared_mem.offset = static_cast<size_t>(std::max(offset, 0));
    params.shared_mem.drm_format_mod = modifier;
    params.shared_mem.stride_w = static_cast<size_t>(stride);
    target.texture = pl_tex_create(m_opengl->gpu, &params);
    if (target.texture == nullptr) {
        ::close(dupFd);
        if (error != nullptr) {
            *error = QStringLiteral("pl_tex_create target import failed (%1 %2 %3 stride=%4 offset=%5)")
                         .arg(
                             drmFormatString(drmFormat),
                             dmabufModifierString(modifier),
                             QStringLiteral("%1x%2").arg(width).arg(height)
                         )
                         .arg(stride)
                         .arg(offset);
        }
        return nullptr;
    }

    auto [inserted, ok] = m_targets.emplace(key, std::move(target));
    Q_UNUSED(ok)
    return &inserted->second;
}

} // namespace quicksov::wallpaper_native
