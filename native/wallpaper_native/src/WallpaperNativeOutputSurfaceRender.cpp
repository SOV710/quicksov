// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperNativeRuntime.hpp"

#include <algorithm>

#include <sys/mman.h>

#include <QDebug>

namespace quicksov::wallpaper_native {

void OutputSurface::render() {
    if (!m_configured || m_surface == nullptr || m_pixelSize.isEmpty()) {
        return;
    }

    wl_buffer *wlBuffer = nullptr;
    void *data = nullptr;
    int width = 0;
    int height = 0;
    int stride = 0;
    int bufferIndex = -1;
    QString usedBackend = QStringLiteral("shm");
    DmaBuffer *usedDmabuf = nullptr;
    ShmBuffer *usedShm = nullptr;
    bool renderedWithGpu = false;

    if (ensureDmabufBuffers()) {
        if (DmaBuffer *buffer = nextFreeDmabufBuffer(); buffer != nullptr) {
            buffer->busy = true;
            wlBuffer = buffer->buffer;
            width = buffer->width;
            height = buffer->height;
            bufferIndex = static_cast<int>(buffer - m_dmabufBuffers.data());
            usedBackend = QStringLiteral("dmabuf");
            usedDmabuf = buffer;
        }
    }

    if (wlBuffer == nullptr) {
        ensureShmBuffers();
        if (ShmBuffer *buffer = nextFreeShmBuffer(); buffer != nullptr) {
            buffer->busy = true;
            wlBuffer = buffer->buffer;
            data = buffer->data;
            width = buffer->width;
            height = buffer->height;
            stride = buffer->stride;
            bufferIndex = static_cast<int>(buffer - m_shmBuffers.data());
            usedBackend = QStringLiteral("shm");
            usedShm = buffer;
        }
    }

    const bool transitionActive = m_transitionDurationMs > 0 && m_transitionClock.isValid();
    GpuCompositor *gpuCompositor = nullptr;
    if (usedDmabuf != nullptr &&
        m_source != nullptr &&
        !transitionActive &&
        m_previousImage.isNull()) {
        QString gpuInitError;
        if (m_renderer->ensureGpuCompositor(&gpuInitError)) {
            gpuCompositor = m_renderer->gpuCompositor();
        } else if (!gpuInitError.isEmpty() && gpuInitError != m_lastGpuError) {
            m_lastGpuError = gpuInitError;
            qDebug().noquote() << kLogPrefix
                               << "gpu fast-path unavailable"
                               << m_outputName
                               << gpuInitError;
        }
    }

    if (usedDmabuf != nullptr &&
        gpuCompositor != nullptr &&
        m_source != nullptr &&
        !transitionActive &&
        m_previousImage.isNull()) {
        const auto hardwareFrame = m_source->hardwareFrameSnapshot();
        QString gpuError;
        renderedWithGpu = gpuCompositor->renderToDmabuf(
            hardwareFrame,
            m_pixelSize,
            m_crop,
            reinterpret_cast<quintptr>(usedDmabuf),
            usedDmabuf->gpuImportFd,
            usedDmabuf->width,
            usedDmabuf->height,
            usedDmabuf->stride,
            usedDmabuf->offset,
            usedDmabuf->format,
            usedDmabuf->modifier,
            m_renderer->conservativeDmabufSyncEnabled(),
            &gpuError
        );
        if (!renderedWithGpu && !gpuError.isEmpty()) {
            if (gpuError != m_lastGpuError) {
                m_lastGpuError = gpuError;
                qDebug().noquote() << kLogPrefix
                                   << "gpu fast-path skipped"
                                   << m_outputName
                                   << gpuError;
            }
        } else if (renderedWithGpu && !m_lastGpuError.isEmpty()) {
            m_lastGpuError.clear();
        }
        if (renderedWithGpu && !m_loggedGpuFastPath) {
            m_loggedGpuFastPath = true;
            qInfo().noquote() << kLogPrefix << "gpu fast-path active" << m_outputName;
        }
    }

    if (usedDmabuf != nullptr && !renderedWithGpu) {
        uint32_t mappedStride = 0;
        void *mapData = nullptr;
        void *mapped = gbm_bo_map(
            usedDmabuf->bo,
            0,
            0,
            static_cast<uint32_t>(usedDmabuf->width),
            static_cast<uint32_t>(usedDmabuf->height),
            GBM_BO_TRANSFER_WRITE,
            &mappedStride,
            &mapData
        );
        if (mapped == nullptr || mapped == MAP_FAILED) {
            usedDmabuf->busy = false;
            m_presentBackendFallbackReason = QStringLiteral("gbm_bo_map_failed");
            wlBuffer = nullptr;
            usedDmabuf = nullptr;
            usedBackend = QStringLiteral("shm");
        } else {
            usedDmabuf->data = mapped;
            usedDmabuf->mapData = mapData;
            usedDmabuf->stride = static_cast<int>(mappedStride);
            data = mapped;
            stride = usedDmabuf->stride;
        }
    }

    if (wlBuffer == nullptr) {
        ensureShmBuffers();
        if (usedShm == nullptr) {
            usedShm = nextFreeShmBuffer();
            if (usedShm != nullptr) {
                usedShm->busy = true;
                wlBuffer = usedShm->buffer;
                data = usedShm->data;
                width = usedShm->width;
                height = usedShm->height;
                stride = usedShm->stride;
                bufferIndex = static_cast<int>(usedShm - m_shmBuffers.data());
                usedBackend = QStringLiteral("shm");
            }
        }
    }

    if (wlBuffer == nullptr || (!renderedWithGpu && data == nullptr)) {
        setCpuFrameRequired(!renderedWithGpu);
        m_bufferStarvedFrames += 1;
        return;
    }

    setCpuFrameRequired(!renderedWithGpu);

    if (!renderedWithGpu) {
        QImage target(
            static_cast<uchar *>(data),
            width,
            height,
            stride,
            QImage::Format_ARGB32_Premultiplied
        );
        target.fill(Qt::black);

        QPainter painter(&target);
        painter.setCompositionMode(QPainter::CompositionMode_SourceOver);

        qreal transitionProgress = 1.0;
        if (m_transitionDurationMs > 0 && m_transitionClock.isValid()) {
            transitionProgress = std::clamp(
                static_cast<qreal>(m_transitionClock.elapsed()) / m_transitionDurationMs,
                0.0,
                1.0
            );
            if (transitionProgress >= 1.0) {
                stopTransition();
            }
        }

        if (!m_previousImage.isNull() && transitionProgress < 1.0) {
            paintImageCover(
                painter,
                m_previousImage,
                m_pixelSize,
                std::nullopt,
                1.0 - transitionProgress
            );
        }

        if (m_source != nullptr) {
            const qreal opacity = (!m_previousImage.isNull() && transitionProgress < 1.0)
                ? transitionProgress
                : 1.0;
            m_source->paint(painter, m_pixelSize, m_crop, opacity);
        }

        painter.end();

        if (usedDmabuf != nullptr && usedDmabuf->mapData != nullptr) {
            gbm_bo_unmap(usedDmabuf->bo, usedDmabuf->mapData);
            usedDmabuf->mapData = nullptr;
            usedDmabuf->data = nullptr;
        }
    }

    wl_surface_set_buffer_scale(m_surface, m_scale);
    wl_surface_attach(m_surface, wlBuffer, 0, 0);
    wl_surface_damage_buffer(m_surface, 0, 0, width, height);
    if (m_frameCallback != nullptr) {
        wl_callback_destroy(m_frameCallback);
        m_frameCallback = nullptr;
    }
    m_frameCallback = wl_surface_frame(m_surface);
    static constexpr wl_callback_listener callbackListener = {
        .done = &OutputSurface::frameDone,
    };
    wl_callback_add_listener(m_frameCallback, &callbackListener, this);
    wl_surface_commit(m_surface);
    flush();

    m_committedFrames += 1;
    m_lastPresentedIndex = bufferIndex;
    m_lastPresentedBackend = usedBackend;
    m_activePresentBackend = usedBackend;
    if (usedBackend == QLatin1String("dmabuf")) {
        m_presentBackendFallbackReason.clear();
    } else if (m_requestedPresentBackend != QLatin1String("shm") &&
               m_presentBackendFallbackReason.isEmpty()) {
        m_presentBackendFallbackReason = m_dmabufFeedbackReady
            ? QStringLiteral("dmabuf_buffer_pending")
            : QStringLiteral("dmabuf_feedback_pending");
    }
    m_dirty = false;
}

void OutputSurface::startTransition(int durationMs) {
    if (m_previousImage.isNull()) {
        return;
    }
    m_transitionDurationMs = std::max(durationMs, 0);
    if (m_transitionDurationMs <= 0) {
        return;
    }
    m_transitionClock.restart();
    if (!m_transitionTimer.isActive()) {
        m_transitionTimer.start();
    }
}

void OutputSurface::stopTransition() {
    m_transitionDurationMs = 0;
    m_previousImage = QImage();
    m_transitionClock.invalidate();
    m_transitionTimer.stop();
}

void OutputSurface::capturePreviousImage() {
    if (m_lastPresentedIndex < 0) {
        m_previousImage = QImage();
        return;
    }

    auto copyImage = [](const uchar *data,
                        int width,
                        int height,
                        int stride,
                        QImage::Format format) {
        if (data == nullptr || width <= 0 || height <= 0 || stride <= 0) {
            return QImage();
        }

        const QImage current(data, width, height, stride, format);
        return current.copy();
    };

    if (m_lastPresentedBackend == QLatin1String("dmabuf")) {
        if (m_lastPresentedIndex >= static_cast<int>(m_dmabufBuffers.size())) {
            m_previousImage = QImage();
            return;
        }

        const DmaBuffer &buffer = m_dmabufBuffers[static_cast<size_t>(m_lastPresentedIndex)];
        if (buffer.bo == nullptr || buffer.width <= 0 || buffer.height <= 0) {
            m_previousImage = QImage();
            return;
        }

        uint32_t mappedStride = 0;
        void *mapData = nullptr;
        void *mapped = gbm_bo_map(
            buffer.bo,
            0,
            0,
            static_cast<uint32_t>(buffer.width),
            static_cast<uint32_t>(buffer.height),
            GBM_BO_TRANSFER_READ,
            &mappedStride,
            &mapData
        );
        if (mapped == nullptr || mapped == MAP_FAILED) {
            m_previousImage = QImage();
            return;
        }

        const QImage::Format format = buffer.format == DRM_FORMAT_XRGB8888
            ? QImage::Format_RGB32
            : QImage::Format_ARGB32_Premultiplied;
        m_previousImage = copyImage(
            static_cast<const uchar *>(mapped),
            buffer.width,
            buffer.height,
            static_cast<int>(mappedStride),
            format
        );
        gbm_bo_unmap(buffer.bo, mapData);
        return;
    }

    if (m_lastPresentedIndex >= static_cast<int>(m_shmBuffers.size())) {
        m_previousImage = QImage();
        return;
    }

    const ShmBuffer &buffer = m_shmBuffers[static_cast<size_t>(m_lastPresentedIndex)];
    m_previousImage = copyImage(
        static_cast<const uchar *>(buffer.data),
        buffer.width,
        buffer.height,
        buffer.stride,
        QImage::Format_ARGB32_Premultiplied
    );
}

} // namespace quicksov::wallpaper_native
