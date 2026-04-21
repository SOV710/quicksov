// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperNativeRuntime.hpp"

#include <algorithm>

#include <QDebug>

namespace quicksov::wallpaper_native {

OutputSurface::OutputSurface(
    WaylandRenderer *renderer,
    uint32_t registryName,
    wl_output *output
)
    : QObject(renderer)
    , m_renderer(renderer)
    , m_registryName(registryName)
    , m_output(output) {
    m_transitionTimer.setInterval(kTransitionFrameMs);
    connect(&m_transitionTimer, &QTimer::timeout, this, [this]() {
        if (m_transitionDurationMs <= 0 ||
            !m_transitionClock.isValid() ||
            m_transitionClock.elapsed() >= m_transitionDurationMs) {
            stopTransition();
        }
        scheduleRender();
    });
}

OutputSurface::~OutputSurface() {
    detachCurrentSourceHint();
    destroySurface();
    if (m_output != nullptr) {
        wl_output_destroy(m_output);
        m_output = nullptr;
    }
}

uint32_t OutputSurface::registryName() const {
    return m_registryName;
}

QString OutputSurface::outputName() const {
    return m_outputName;
}

SourceSession *OutputSurface::boundSource() const {
    return m_source.get();
}

OutputSurface::StatsSnapshot OutputSurface::statsSnapshot() const {
    return StatsSnapshot{
        .outputName = m_outputName,
        .sourceId = (m_source != nullptr) ? m_source->spec().id : QString(),
        .requestedPresentBackend = m_requestedPresentBackend,
        .resolvedPresentBackend = m_activePresentBackend,
        .presentBackendFallbackReason = m_presentBackendFallbackReason,
        .logicalSize = m_logicalSize,
        .pixelSize = m_pixelSize,
        .committedFrames = m_committedFrames,
        .presentedFrames = m_presentedFrames,
        .bufferStarvedFrames = m_bufferStarvedFrames,
        .configured = m_configured,
    };
}

void OutputSurface::setScale(int scale) {
    if (m_scale == scale) {
        return;
    }
    m_scale = std::max(1, scale);
    if (m_logicalSize.isValid()) {
        m_pixelSize = QSize(m_logicalSize.width() * m_scale, m_logicalSize.height() * m_scale);
    }
    destroyBuffers();
    updateVideoHint();
    scheduleRender();
}

void OutputSurface::setName(const QString &name) {
    if (m_outputName == name) {
        return;
    }
    m_outputName = name;
    qInfo().noquote() << kLogPrefix << "output discovered" << m_outputName;
    m_renderer->rebindOutputs();
}

void OutputSurface::createSurface() {
    if (m_surface != nullptr ||
        m_renderer->compositor() == nullptr ||
        m_renderer->layerShell() == nullptr) {
        return;
    }

    m_surface = wl_compositor_create_surface(m_renderer->compositor());
    if (m_surface == nullptr) {
        qWarning().noquote() << kLogPrefix << "failed to create wl_surface";
        return;
    }

    m_layerSurface = zwlr_layer_shell_v1_get_layer_surface(
        m_renderer->layerShell(),
        m_surface,
        m_output,
        ZWLR_LAYER_SHELL_V1_LAYER_BACKGROUND,
        kNamespace
    );
    if (m_layerSurface == nullptr) {
        qWarning().noquote() << kLogPrefix << "failed to create layer surface";
        wl_surface_destroy(m_surface);
        m_surface = nullptr;
        return;
    }

    static constexpr zwlr_layer_surface_v1_listener layerListener = {
        .configure = &OutputSurface::layerSurfaceConfigure,
        .closed = &OutputSurface::layerSurfaceClosed,
    };
    zwlr_layer_surface_v1_add_listener(m_layerSurface, &layerListener, this);
    zwlr_layer_surface_v1_set_anchor(
        m_layerSurface,
        ZWLR_LAYER_SURFACE_V1_ANCHOR_TOP |
            ZWLR_LAYER_SURFACE_V1_ANCHOR_BOTTOM |
            ZWLR_LAYER_SURFACE_V1_ANCHOR_LEFT |
            ZWLR_LAYER_SURFACE_V1_ANCHOR_RIGHT
    );
    zwlr_layer_surface_v1_set_size(m_layerSurface, 0, 0);
    zwlr_layer_surface_v1_set_keyboard_interactivity(
        m_layerSurface,
        ZWLR_LAYER_SURFACE_V1_KEYBOARD_INTERACTIVITY_NONE
    );
    zwlr_layer_surface_v1_set_exclusive_zone(m_layerSurface, -1);

    wl_region *region = wl_compositor_create_region(m_renderer->compositor());
    wl_surface_set_input_region(m_surface, region);
    wl_region_destroy(region);

    if (m_renderer->linuxDmabuf() != nullptr && m_renderer->dmabufVersion() >= 4) {
        m_surfaceDmabufFeedback =
            zwp_linux_dmabuf_v1_get_surface_feedback(m_renderer->linuxDmabuf(), m_surface);
        static constexpr zwp_linux_dmabuf_feedback_v1_listener feedbackListener = {
            .done = &OutputSurface::dmabufFeedbackDone,
            .format_table = &OutputSurface::dmabufFeedbackFormatTable,
            .main_device = &OutputSurface::dmabufFeedbackMainDevice,
            .tranche_done = &OutputSurface::dmabufFeedbackTrancheDone,
            .tranche_target_device = &OutputSurface::dmabufFeedbackTrancheTargetDevice,
            .tranche_formats = &OutputSurface::dmabufFeedbackTrancheFormats,
            .tranche_flags = &OutputSurface::dmabufFeedbackTrancheFlags,
        };
        zwp_linux_dmabuf_feedback_v1_add_listener(
            m_surfaceDmabufFeedback,
            &feedbackListener,
            this
        );
    }

    wl_surface_commit(m_surface);
    flush();
}

void OutputSurface::destroySurface() {
    if (m_frameCallback != nullptr) {
        wl_callback_destroy(m_frameCallback);
        m_frameCallback = nullptr;
    }
    destroyBuffers();
    if (m_surfaceDmabufFeedback != nullptr) {
        zwp_linux_dmabuf_feedback_v1_destroy(m_surfaceDmabufFeedback);
        m_surfaceDmabufFeedback = nullptr;
    }
    m_dmabufFormatTable.clear();
    m_surfaceDmabufFormats.clear();
    m_surfaceDmabufTranches.clear();
    m_pendingDmabufTranche = DmabufTranche{};
    m_dmabufFeedbackReady = false;
    m_dmabufDisabled = false;
    if (m_layerSurface != nullptr) {
        zwlr_layer_surface_v1_destroy(m_layerSurface);
        m_layerSurface = nullptr;
    }
    if (m_surface != nullptr) {
        wl_surface_destroy(m_surface);
        m_surface = nullptr;
    }
    m_configured = false;
    m_dirty = false;
}

void OutputSurface::applySnapshot(
    const SnapshotModel &snapshot,
    const std::map<QString, std::shared_ptr<SourceSession>> &sources
) {
    const auto presentBackend = m_renderer->resolvePresentBackend(snapshot.presentBackend);
    m_requestedPresentBackend = presentBackend.requested;
    m_targetPresentBackend = presentBackend.resolved;
    m_presentBackendFallbackReason = presentBackend.fallbackReason;
    if (m_targetPresentBackend == QLatin1String("shm")) {
        m_activePresentBackend = QStringLiteral("shm");
        destroyDmabufBuffers();
        m_dmabufDisabled = false;
    }

    std::shared_ptr<SourceSession> nextSource;
    std::optional<CropRect> nextCrop;

    const auto viewIt = snapshot.views.find(m_outputName);
    if (viewIt != snapshot.views.end()) {
        const auto sourceIt = sources.find(viewIt->source);
        if (sourceIt != sources.end()) {
            nextSource = sourceIt->second;
            nextCrop = viewIt->crop;
        }
    } else if (!snapshot.fallbackSource.isEmpty()) {
        const auto sourceIt = sources.find(snapshot.fallbackSource);
        if (sourceIt != sources.end()) {
            nextSource = sourceIt->second;
        }
    }

    setBinding(std::move(nextSource), nextCrop, snapshot.transitionDurationMs);
}

void OutputSurface::scheduleRender() {
    m_dirty = true;
    if (m_surface == nullptr || !m_configured) {
        return;
    }
    if (m_frameCallback == nullptr) {
        render();
    }
}

void OutputSurface::onFrameCallbackDone() {
    m_presentedFrames += 1;
    if (m_frameCallback != nullptr) {
        wl_callback_destroy(m_frameCallback);
        m_frameCallback = nullptr;
    }
    if (m_dirty) {
        render();
    }
}

void OutputSurface::onBufferReleased() {
    if (m_dirty && m_frameCallback == nullptr) {
        render();
    }
}

void OutputSurface::handleClosed() {
    qWarning().noquote() << kLogPrefix << "layer surface closed" << m_outputName;
    destroySurface();
}

void OutputSurface::handleConfigure(uint32_t serial, uint32_t width, uint32_t height) {
    if (m_layerSurface == nullptr) {
        return;
    }
    zwlr_layer_surface_v1_ack_configure(m_layerSurface, serial);

    if (width > 0 && height > 0) {
        m_logicalSize = QSize(static_cast<int>(width), static_cast<int>(height));
        m_pixelSize = QSize(m_logicalSize.width() * m_scale, m_logicalSize.height() * m_scale);
        destroyBuffers();
        updateVideoHint();
    }

    m_configured = true;
    scheduleRender();
}

void OutputSurface::updateVideoHint() {
    if (m_source != nullptr) {
        m_source->updateRenderHint(this, m_pixelSize, m_cpuFrameRequired);
    }
}

void OutputSurface::resetGpuResources() {
    m_loggedGpuFastPath = false;
    m_lastGpuError.clear();
    m_dmabufDisabled = false;
    destroyDmabufBuffers();
    scheduleRender();
}

void OutputSurface::setCpuFrameRequired(bool required) {
    if (m_cpuFrameRequired == required) {
        return;
    }

    m_cpuFrameRequired = required;
    updateVideoHint();
}

void OutputSurface::setBinding(
    std::shared_ptr<SourceSession> source,
    std::optional<CropRect> crop,
    int transitionMs
) {
    const bool sourceChanged = m_source.get() != source.get();
    const bool cropChanged = m_crop != crop;
    if (!sourceChanged && !cropChanged) {
        return;
    }

    if (sourceChanged) {
        capturePreviousImage();
        detachCurrentSourceHint();
        m_source = std::move(source);
        m_cpuFrameRequired = true;
        updateVideoHint();
    } else {
        m_source = std::move(source);
    }

    m_crop = crop;
    if (sourceChanged && transitionMs > 0 && !m_previousImage.isNull()) {
        startTransition(transitionMs);
    } else if (sourceChanged) {
        stopTransition();
    }
    scheduleRender();
}

void OutputSurface::detachCurrentSourceHint() {
    if (m_source != nullptr) {
        m_source->removeRenderHint(this);
    }
}

void OutputSurface::flush() {
    m_renderer->flush();
}

void OutputSurface::layerSurfaceConfigure(
    void *data,
    zwlr_layer_surface_v1 *,
    uint32_t serial,
    uint32_t width,
    uint32_t height
) {
    static_cast<OutputSurface *>(data)->handleConfigure(serial, width, height);
}

void OutputSurface::layerSurfaceClosed(void *data, zwlr_layer_surface_v1 *) {
    static_cast<OutputSurface *>(data)->handleClosed();
}

void OutputSurface::frameDone(void *data, wl_callback *, uint32_t) {
    static_cast<OutputSurface *>(data)->onFrameCallbackDone();
}

void OutputSurface::shmBufferReleased(void *data, wl_buffer *) {
    auto *buffer = static_cast<ShmBuffer *>(data);
    buffer->busy = false;
    if (buffer->owner != nullptr) {
        buffer->owner->onBufferReleased();
    }
}

void OutputSurface::dmabufBufferReleased(void *data, wl_buffer *) {
    auto *buffer = static_cast<DmaBuffer *>(data);
    buffer->busy = false;
    if (buffer->owner != nullptr) {
        buffer->owner->onBufferReleased();
    }
}

wl_output *OutputSurface::output() const {
    return m_output;
}

} // namespace quicksov::wallpaper_native
