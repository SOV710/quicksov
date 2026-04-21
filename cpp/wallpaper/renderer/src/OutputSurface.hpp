// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "SnapshotModel.hpp"

#include <array>
#include <map>
#include <memory>

#include <QByteArray>
#include <QElapsedTimer>
#include <QObject>
#include <QTimer>

extern "C" {
#include <gbm.h>
#include <wayland-client.h>
}

#define namespace namespace_
#include "wlr-layer-shell-unstable-v1-client-protocol.h"
#undef namespace
#include "linux-dmabuf-v1-client-protocol.h"

namespace quicksov::wallpaper::renderer {

class SourceSession;
class WaylandRenderer;

class OutputSurface final : public QObject {
    Q_OBJECT

public:
    struct StatsSnapshot {
        QString outputName;
        QString sourceId;
        QString requestedPresentBackend;
        QString resolvedPresentBackend;
        QString presentBackendFallbackReason;
        QSize logicalSize;
        QSize pixelSize;
        quint64 committedFrames = 0;
        quint64 presentedFrames = 0;
        quint64 bufferStarvedFrames = 0;
        bool configured = false;
    };

    OutputSurface(WaylandRenderer *renderer, uint32_t registryName, wl_output *output);
    ~OutputSurface() override;

    [[nodiscard]] uint32_t registryName() const;
    [[nodiscard]] QString outputName() const;
    [[nodiscard]] SourceSession *boundSource() const;
    [[nodiscard]] StatsSnapshot statsSnapshot() const;

    void setScale(int scale);
    void setName(const QString &name);
    void createSurface();
    void destroySurface();
    void applySnapshot(
        const SnapshotModel &snapshot,
        const std::map<QString, std::shared_ptr<SourceSession>> &sources
    );
    void scheduleRender();
    void onFrameCallbackDone();
    void onBufferReleased();
    void handleClosed();
    void handleConfigure(uint32_t serial, uint32_t width, uint32_t height);
    void updateVideoHint();
    void resetGpuResources();

    [[nodiscard]] wl_output *output() const;

private:
    struct ShmBuffer {
        OutputSurface *owner = nullptr;
        wl_buffer *buffer = nullptr;
        void *data = nullptr;
        size_t bytes = 0;
        int width = 0;
        int height = 0;
        int stride = 0;
        bool busy = false;
    };

    struct DmaBuffer {
        OutputSurface *owner = nullptr;
        wl_buffer *buffer = nullptr;
        zwp_linux_buffer_params_v1 *params = nullptr;
        gbm_bo *bo = nullptr;
        int gpuImportFd = -1;
        void *data = nullptr;
        void *mapData = nullptr;
        int width = 0;
        int height = 0;
        int stride = 0;
        int offset = 0;
        uint32_t format = kDmabufDrmFormat;
        uint64_t modifier = DRM_FORMAT_MOD_INVALID;
        bool busy = false;
        bool pending = false;
    };

    void setBinding(std::shared_ptr<SourceSession> source, std::optional<CropRect> crop, int transitionMs);
    void detachCurrentSourceHint();
    void setCpuFrameRequired(bool required);
    void ensureShmBuffers();
    bool ensureDmabufBuffers();
    void destroyBuffers();
    void destroyShmBuffers();
    void destroyDmabufBuffers();
    ShmBuffer *nextFreeShmBuffer();
    DmaBuffer *nextFreeDmabufBuffer();
    bool createDmabufBuffer(DmaBuffer &buffer);
    void releaseDmabufBuffer(DmaBuffer &buffer);
    void disableDmabuf(const QString &reason);
    bool supportsDmabufModifier(uint32_t format, uint64_t modifier) const;
    std::vector<uint64_t> supportedDmabufModifiers(uint32_t format) const;
    std::vector<uint64_t> supportedDmabufModifiersForDevice(uint32_t format, dev_t targetDevice) const;
    std::vector<uint64_t> dmabufModifierCandidates(uint32_t format) const;
    void render();
    void startTransition(int durationMs);
    void stopTransition();
    void capturePreviousImage();
    void flush();
    void onDmabufBufferCreated();
    void onDmabufBufferFailed(DmaBuffer *buffer);

    static void layerSurfaceConfigure(
        void *data,
        zwlr_layer_surface_v1 *layerSurface,
        uint32_t serial,
        uint32_t width,
        uint32_t height
    );
    static void layerSurfaceClosed(void *data, zwlr_layer_surface_v1 *layerSurface);
    static void frameDone(void *data, wl_callback *callback, uint32_t time);
    static void shmBufferReleased(void *data, wl_buffer *buffer);
    static void dmabufBufferReleased(void *data, wl_buffer *buffer);
    static void dmabufParamsCreated(
        void *data,
        zwp_linux_buffer_params_v1 *params,
        wl_buffer *buffer
    );
    static void dmabufParamsFailed(void *data, zwp_linux_buffer_params_v1 *params);
    static void dmabufFeedbackDone(void *data, zwp_linux_dmabuf_feedback_v1 *feedback);
    static void dmabufFeedbackFormatTable(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        int32_t fd,
        uint32_t size
    );
    static void dmabufFeedbackMainDevice(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        wl_array *device
    );
    static void dmabufFeedbackTrancheDone(void *data, zwp_linux_dmabuf_feedback_v1 *feedback);
    static void dmabufFeedbackTrancheTargetDevice(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        wl_array *device
    );
    static void dmabufFeedbackTrancheFormats(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        wl_array *indices
    );
    static void dmabufFeedbackTrancheFlags(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *feedback,
        uint32_t flags
    );

    WaylandRenderer *m_renderer = nullptr;
    uint32_t m_registryName = 0;
    wl_output *m_output = nullptr;
    wl_surface *m_surface = nullptr;
    zwlr_layer_surface_v1 *m_layerSurface = nullptr;
    zwp_linux_dmabuf_feedback_v1 *m_surfaceDmabufFeedback = nullptr;
    wl_callback *m_frameCallback = nullptr;
    QString m_outputName;
    QString m_requestedPresentBackend = QStringLiteral("auto");
    QString m_targetPresentBackend = QStringLiteral("shm");
    QString m_activePresentBackend = QStringLiteral("shm");
    QString m_presentBackendFallbackReason;
    bool m_cpuFrameRequired = true;
    int m_scale = 1;
    QSize m_logicalSize;
    QSize m_pixelSize;
    bool m_configured = false;
    bool m_dirty = false;
    std::shared_ptr<SourceSession> m_source;
    std::optional<CropRect> m_crop;
    std::array<ShmBuffer, 2> m_shmBuffers;
    std::array<DmaBuffer, 2> m_dmabufBuffers;
    int m_lastPresentedIndex = -1;
    QString m_lastPresentedBackend = QStringLiteral("shm");
    QImage m_previousImage;
    QString m_lastGpuError;
    bool m_loggedGpuFastPath = false;
    QElapsedTimer m_transitionClock;
    QTimer m_transitionTimer;
    int m_transitionDurationMs = 0;
    quint64 m_committedFrames = 0;
    quint64 m_presentedFrames = 0;
    quint64 m_bufferStarvedFrames = 0;
    QByteArray m_dmabufFormatTable;
    std::vector<DmabufFormatModifier> m_surfaceDmabufFormats;
    std::vector<DmabufTranche> m_surfaceDmabufTranches;
    DmabufTranche m_pendingDmabufTranche;
    QString m_lastDmabufAllocFailureSignature;
    quint64 m_dmabufAllocFailureRepeats = 0;
    bool m_dmabufFeedbackReady = false;
    bool m_dmabufDisabled = false;
};

} // namespace quicksov::wallpaper::renderer
