// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "SnapshotModel.hpp"

#include <map>
#include <memory>
#include <optional>

#include <QHash>
#include <QObject>
#include <QSocketNotifier>
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

class GpuCompositor;
class OutputSurface;
class SourceSession;

class WaylandRenderer final : public QObject {
    Q_OBJECT

public:
    struct PresentBackendSelection {
        QString requested;
        QString resolved;
        QString fallbackReason;
    };

    explicit WaylandRenderer(QObject *parent = nullptr);
    ~WaylandRenderer() override;

    bool initialize(QString *error);
    void applySnapshot(const SnapshotModel &snapshot);

    [[nodiscard]] wl_compositor *compositor() const;
    [[nodiscard]] wl_shm *shm() const;
    [[nodiscard]] zwp_linux_dmabuf_v1 *linuxDmabuf() const;
    [[nodiscard]] GpuCompositor *gpuCompositor() const;
    [[nodiscard]] bool ensureGpuCompositor(QString *reason);
    [[nodiscard]] bool ensureGbmDevice(QString *reason);
    [[nodiscard]] gbm_device *gbmDevice() const;
    [[nodiscard]] bool ensureDmabufAllocationDevice(QString *reason);
    [[nodiscard]] gbm_device *dmabufAllocationDevice() const;
    [[nodiscard]] PresentBackendSelection resolvePresentBackend(const QString &requested) const;
    [[nodiscard]] bool dmabufAdvertised() const;
    [[nodiscard]] uint32_t dmabufVersion() const;
    [[nodiscard]] quint64 dmabufFormatCount() const;
    [[nodiscard]] quint64 dmabufModifierCount() const;
    [[nodiscard]] std::optional<dev_t> dmabufMainDevice() const;
    [[nodiscard]] QString gbmDevicePath() const;
    [[nodiscard]] QString dmabufAllocationDevicePath() const;
    [[nodiscard]] bool conservativeDmabufSyncEnabled() const;
    [[nodiscard]] zwlr_layer_shell_v1 *layerShell() const;
    void flush();
    void rebindOutputs();

signals:
    void fatalError(const QString &message);

private:
    [[nodiscard]] QVector<GpuDeviceInfo> gpuDevices() const;
    [[nodiscard]] QString compositorDevicePath() const;
    [[nodiscard]] QString resolveRenderDevicePath(const QVector<GpuDeviceInfo> &devices) const;
    [[nodiscard]] QString resolvePresentDevicePath(
        const QVector<GpuDeviceInfo> &devices,
        const QString &renderDevicePath
    ) const;
    [[nodiscard]] QString resolveDecodeDevicePath(
        const QVector<GpuDeviceInfo> &devices,
        const QString &renderDevicePath
    ) const;
    [[nodiscard]] QStringList resolveDecodeBackendOrder(
        const QVector<GpuDeviceInfo> &devices,
        const QString &decodeDevicePath
    ) const;
    [[nodiscard]] QString describeGpuPath(
        const QVector<GpuDeviceInfo> &devices,
        const QString &path
    ) const;
    void resetGpuPipeline();
    void logTelemetry();
    static void registryGlobal(
        void *data,
        wl_registry *registry,
        uint32_t name,
        const char *interface,
        uint32_t version
    );
    static void registryGlobalRemove(void *data, wl_registry *, uint32_t name);
    static void dmabufFormat(void *data, zwp_linux_dmabuf_v1 *, uint32_t);
    static void dmabufModifier(void *data, zwp_linux_dmabuf_v1 *, uint32_t, uint32_t, uint32_t);
    static void defaultFeedbackDone(void *, zwp_linux_dmabuf_feedback_v1 *);
    static void defaultFeedbackFormatTable(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        int32_t fd,
        uint32_t
    );
    static void defaultFeedbackMainDevice(
        void *data,
        zwp_linux_dmabuf_feedback_v1 *,
        wl_array *device
    );
    static void defaultFeedbackTrancheDone(void *, zwp_linux_dmabuf_feedback_v1 *);
    static void defaultFeedbackTrancheTargetDevice(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        wl_array *
    );
    static void defaultFeedbackTrancheFormats(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        wl_array *
    );
    static void defaultFeedbackTrancheFlags(
        void *,
        zwp_linux_dmabuf_feedback_v1 *,
        uint32_t
    );
    void onSourceUpdated(SourceSession *source);

    wl_display *m_display = nullptr;
    wl_registry *m_registry = nullptr;
    wl_compositor *m_compositor = nullptr;
    wl_shm *m_shm = nullptr;
    zwp_linux_dmabuf_v1 *m_linuxDmabuf = nullptr;
    zwp_linux_dmabuf_feedback_v1 *m_defaultDmabufFeedback = nullptr;
    zwlr_layer_shell_v1 *m_layerShell = nullptr;
    QSocketNotifier *m_displayNotifier = nullptr;
    QTimer m_telemetryTimer;
    SnapshotModel m_snapshot;
    std::map<uint32_t, std::unique_ptr<OutputSurface>> m_outputs;
    std::map<QString, std::shared_ptr<SourceSession>> m_sources;
    QHash<QString, quint64> m_prevDecodedFrames;
    QHash<QString, quint64> m_prevCommittedFrames;
    QHash<QString, quint64> m_prevPresentedFrames;
    QHash<QString, quint64> m_prevBufferStarvedFrames;
    bool m_dmabufAdvertised = false;
    uint32_t m_dmabufVersion = 0;
    quint64 m_dmabufFormatCount = 0;
    quint64 m_dmabufModifierCount = 0;
    bool m_hasSnapshot = false;
    std::optional<dev_t> m_dmabufMainDevice;
    gbm_device *m_gbmDevice = nullptr;
    int m_gbmDeviceFd = -1;
    bool m_gbmDeviceAttempted = false;
    QString m_gbmDevicePath;
    QString m_gbmDeviceFailureReason;
    gbm_device *m_dmabufAllocationDevice = nullptr;
    int m_dmabufAllocationDeviceFd = -1;
    bool m_dmabufAllocationDeviceAttempted = false;
    QString m_dmabufAllocationDevicePath;
    QString m_dmabufAllocationDeviceFailureReason;
    std::unique_ptr<GpuCompositor> m_gpuCompositor;
    bool m_gpuCompositorAttempted = false;
    QString m_gpuCompositorFailureReason;
    QString m_effectiveCompositorDevicePath;
    QString m_effectiveRenderDevicePath;
    QString m_effectivePresentDevicePath;
    QString m_effectiveDecodeDevicePath;
    QStringList m_effectiveDecodeBackendOrder;
};

} // namespace quicksov::wallpaper::renderer
