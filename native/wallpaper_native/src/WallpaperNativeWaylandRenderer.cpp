// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperNativeRuntime.hpp"

#include <algorithm>
#include <cerrno>
#include <cstring>

#include <fcntl.h>
#include <unistd.h>

namespace quicksov::wallpaper_native {

WaylandRenderer::WaylandRenderer(QObject *parent)
    : QObject(parent) {}

WaylandRenderer::~WaylandRenderer() {
    if (m_displayNotifier != nullptr) {
        delete m_displayNotifier;
        m_displayNotifier = nullptr;
    }

    m_outputs.clear();

    if (m_defaultDmabufFeedback != nullptr) {
        zwp_linux_dmabuf_feedback_v1_destroy(m_defaultDmabufFeedback);
        m_defaultDmabufFeedback = nullptr;
    }
    if (m_layerShell != nullptr) {
        zwlr_layer_shell_v1_destroy(m_layerShell);
        m_layerShell = nullptr;
    }
    if (m_linuxDmabuf != nullptr) {
        zwp_linux_dmabuf_v1_destroy(m_linuxDmabuf);
        m_linuxDmabuf = nullptr;
    }
    if (m_gbmDevice != nullptr) {
        gbm_device_destroy(m_gbmDevice);
        m_gbmDevice = nullptr;
    }
    if (m_gbmDeviceFd >= 0) {
        ::close(m_gbmDeviceFd);
        m_gbmDeviceFd = -1;
    }
    m_gpuCompositor.reset();
    if (m_shm != nullptr) {
        wl_shm_destroy(m_shm);
        m_shm = nullptr;
    }
    if (m_compositor != nullptr) {
        wl_compositor_destroy(m_compositor);
        m_compositor = nullptr;
    }
    if (m_registry != nullptr) {
        wl_registry_destroy(m_registry);
        m_registry = nullptr;
    }
    if (m_display != nullptr) {
        wl_display_disconnect(m_display);
        m_display = nullptr;
    }
}

bool WaylandRenderer::initialize(QString *error) {
    m_display = wl_display_connect(nullptr);
    if (m_display == nullptr) {
        *error = QStringLiteral("wl_display_connect failed");
        return false;
    }

    m_registry = wl_display_get_registry(m_display);
    if (m_registry == nullptr) {
        *error = QStringLiteral("wl_display_get_registry failed");
        return false;
    }

    static constexpr wl_registry_listener registryListener = {
        .global = &WaylandRenderer::registryGlobal,
        .global_remove = &WaylandRenderer::registryGlobalRemove,
    };
    wl_registry_add_listener(m_registry, &registryListener, this);

    if (wl_display_roundtrip(m_display) < 0 || wl_display_roundtrip(m_display) < 0) {
        *error = QStringLiteral("wl_display_roundtrip failed");
        return false;
    }

    if (m_compositor == nullptr) {
        *error = QStringLiteral("wl_compositor missing");
        return false;
    }
    if (m_shm == nullptr) {
        *error = QStringLiteral("wl_shm missing");
        return false;
    }
    if (m_layerShell == nullptr) {
        *error = QStringLiteral("zwlr_layer_shell_v1 missing");
        return false;
    }

    for (auto &entry : m_outputs) {
        entry.second->createSurface();
    }

    const int fd = wl_display_get_fd(m_display);
    if (fd < 0) {
        *error = QStringLiteral("wl_display_get_fd failed");
        return false;
    }

    m_displayNotifier = new QSocketNotifier(fd, QSocketNotifier::Read, this);
    connect(m_displayNotifier, &QSocketNotifier::activated, this, [this]() {
        if (wl_display_dispatch(m_display) < 0) {
            emit fatalError(QStringLiteral("wl_display_dispatch failed"));
            return;
        }
        wl_display_flush(m_display);
    });

    m_telemetryTimer.setInterval(5000);
    connect(&m_telemetryTimer, &QTimer::timeout, this, &WaylandRenderer::logTelemetry);
    m_telemetryTimer.start();

    qInfo().noquote() << kLogPrefix << "wayland renderer initialized"
                      << "outputs =" << static_cast<int>(m_outputs.size())
                      << "dmabuf_advertised =" << m_dmabufAdvertised
                      << "dmabuf_version =" << m_dmabufVersion
                      << "dmabuf_formats =" << m_dmabufFormatCount
                      << "dmabuf_modifiers =" << m_dmabufModifierCount;
    return true;
}

void WaylandRenderer::applySnapshot(const SnapshotModel &snapshot) {
    m_snapshot = snapshot;
    m_hasSnapshot = true;
    const QVector<GpuDeviceInfo> devices = gpuDevices();
    const QString compositorPath = compositorDevicePath();
    const QString renderDevicePath = resolveRenderDevicePath(devices);
    const QString presentDevicePath = resolvePresentDevicePath(devices, renderDevicePath);
    const QString decodeDevicePath = resolveDecodeDevicePath(devices, renderDevicePath);
    const QStringList decodeBackendOrder = resolveDecodeBackendOrder(devices, decodeDevicePath);
    const bool presentDeviceChanged = m_effectivePresentDevicePath != presentDevicePath;

    if (!m_effectiveRenderDevicePath.isEmpty() &&
        !renderDevicePath.isEmpty() &&
        m_effectiveRenderDevicePath != renderDevicePath) {
        qInfo().noquote() << kLogPrefix
                          << "wallpaper render device changed; resetting GPU pipeline"
                          << "old =" << describeGpuPath(devices, m_effectiveRenderDevicePath)
                          << "new =" << describeGpuPath(devices, renderDevicePath);
        resetGpuPipeline();
        for (auto &entry : m_outputs) {
            entry.second->resetGpuResources();
        }
    }

    if (presentDeviceChanged && m_dmabufAllocationDevice != nullptr) {
        gbm_device_destroy(m_dmabufAllocationDevice);
        m_dmabufAllocationDevice = nullptr;
        if (m_dmabufAllocationDeviceFd >= 0) {
            ::close(m_dmabufAllocationDeviceFd);
            m_dmabufAllocationDeviceFd = -1;
        }
        m_dmabufAllocationDeviceAttempted = false;
        m_dmabufAllocationDeviceFailureReason.clear();
        m_dmabufAllocationDevicePath.clear();
        for (auto &entry : m_outputs) {
            entry.second->resetGpuResources();
        }
    }

    const bool gpuSelectionChanged =
        m_effectiveCompositorDevicePath != compositorPath ||
        m_effectiveRenderDevicePath != renderDevicePath ||
        m_effectivePresentDevicePath != presentDevicePath ||
        m_effectiveDecodeDevicePath != decodeDevicePath ||
        m_effectiveDecodeBackendOrder != decodeBackendOrder;

    m_effectiveCompositorDevicePath = compositorPath;
    m_effectiveRenderDevicePath = renderDevicePath;
    m_effectivePresentDevicePath = presentDevicePath;
    m_effectiveDecodeDevicePath = decodeDevicePath;
    m_effectiveDecodeBackendOrder = decodeBackendOrder;

    if (gpuSelectionChanged) {
        qInfo().noquote() << kLogPrefix
                          << "wallpaper GPU selection"
                          << "render_policy =" << snapshot.renderDevicePolicy
                          << "decode_policy =" << snapshot.decodeDevicePolicy
                          << "allow_cross_gpu =" << snapshot.allowCrossGpu
                          << "compositor =" << describeGpuPath(devices, compositorPath)
                          << "render =" << describeGpuPath(devices, renderDevicePath)
                          << "present =" << describeGpuPath(devices, presentDevicePath)
                          << "decode =" << describeGpuPath(devices, decodeDevicePath)
                          << "candidates =" << describeGpuDevices(devices)
                          << "decode_backends =" << decodeBackendOrder.join(QLatin1Char(','));
    }

    for (const auto &[id, spec] : snapshot.sources.asKeyValueRange()) {
        auto it = m_sources.find(id);
        if (it != m_sources.end()) {
            if (it->second->matches(spec, decodeBackendOrder, decodeDevicePath)) {
                continue;
            }
        }

        auto session = std::make_shared<SourceSession>(spec, decodeBackendOrder, decodeDevicePath);
        connect(session.get(), &SourceSession::updated, this, [this, raw = session.get()]() {
            onSourceUpdated(raw);
        });
        m_sources[id] = std::move(session);
    }

    for (auto &entry : m_outputs) {
        entry.second->applySnapshot(m_snapshot, m_sources);
    }

    for (auto it = m_sources.begin(); it != m_sources.end();) {
        if (!m_snapshot.sources.contains(it->first)) {
            it = m_sources.erase(it);
        } else {
            ++it;
        }
    }

    flush();
}

wl_compositor *WaylandRenderer::compositor() const {
    return m_compositor;
}

wl_shm *WaylandRenderer::shm() const {
    return m_shm;
}

zwp_linux_dmabuf_v1 *WaylandRenderer::linuxDmabuf() const {
    return m_linuxDmabuf;
}

GpuCompositor *WaylandRenderer::gpuCompositor() const {
    return m_gpuCompositor.get();
}

bool WaylandRenderer::ensureGpuCompositor(QString *reason) {
    if (m_gpuCompositor != nullptr) {
        if (reason != nullptr) {
            reason->clear();
        }
        return true;
    }

    if (m_gpuCompositorAttempted) {
        if (reason != nullptr) {
            *reason = m_gpuCompositorFailureReason;
        }
        return false;
    }

    QString gbmReason;
    if (!ensureGbmDevice(&gbmReason) || m_gbmDevice == nullptr) {
        m_gpuCompositorAttempted = true;
        m_gpuCompositorFailureReason =
            QStringLiteral("gbm_device_unavailable:%1").arg(gbmReason);
        if (reason != nullptr) {
            *reason = m_gpuCompositorFailureReason;
        }
        return false;
    }

    auto compositor = std::make_unique<GpuCompositor>();
    QString gpuError;
    if (!compositor->initialize(m_gbmDevice, &gpuError)) {
        m_gpuCompositorAttempted = true;
        m_gpuCompositorFailureReason = gpuError;
        qWarning().noquote() << kLogPrefix << "gpu compositor unavailable:" << gpuError;
        if (reason != nullptr) {
            *reason = m_gpuCompositorFailureReason;
        }
        return false;
    }

    m_gpuCompositor = std::move(compositor);
    m_gpuCompositorFailureReason.clear();
    if (reason != nullptr) {
        reason->clear();
    }
    return true;
}

bool WaylandRenderer::ensureGbmDevice(QString *reason) {
    if (!m_hasSnapshot) {
        if (reason != nullptr) {
            *reason = QStringLiteral("snapshot_not_ready");
        }
        return false;
    }

    if (!m_dmabufAdvertised || m_linuxDmabuf == nullptr) {
        if (reason != nullptr) {
            *reason = QStringLiteral("dmabuf_not_advertised");
        }
        return false;
    }

    const QVector<GpuDeviceInfo> devices = gpuDevices();
    const QString selectedPath = resolveRenderDevicePath(devices);

    if (m_gbmDevice != nullptr && m_gbmDevicePath == selectedPath) {
        if (reason != nullptr) {
            reason->clear();
        }
        return true;
    }

    if (m_gbmDevice != nullptr && m_gbmDevicePath != selectedPath) {
        qInfo().noquote() << kLogPrefix
                          << "reinitializing GBM device for updated GPU selection"
                          << "old =" << describeGpuPath(devices, m_gbmDevicePath)
                          << "new =" << describeGpuPath(devices, selectedPath);
        resetGpuPipeline();
        for (auto &entry : m_outputs) {
            entry.second->resetGpuResources();
        }
    }

    if (m_gbmDeviceAttempted) {
        if (reason != nullptr) {
            *reason = m_gbmDeviceFailureReason;
        }
        return false;
    }

    m_gbmDeviceAttempted = true;

    QString path = selectedPath;
    if (path.isEmpty()) {
        path = firstExistingDrmNode(
            QStringList{QStringLiteral("renderD*"), QStringLiteral("card*")}
        );
    }
    if (path.isEmpty()) {
        m_gbmDeviceFailureReason = QStringLiteral("drm_node_missing");
        if (reason != nullptr) {
            *reason = m_gbmDeviceFailureReason;
        }
        return false;
    }

    const int fd = ::open(path.toUtf8().constData(), O_RDWR | O_CLOEXEC);
    if (fd < 0) {
        m_gbmDeviceFailureReason = QStringLiteral("drm_node_open_failed");
        qWarning().noquote() << kLogPrefix << "failed to open DRM node"
                             << path << std::strerror(errno);
        if (reason != nullptr) {
            *reason = m_gbmDeviceFailureReason;
        }
        return false;
    }

    gbm_device *device = gbm_create_device(fd);
    if (device == nullptr) {
        ::close(fd);
        m_gbmDeviceFailureReason = QStringLiteral("gbm_device_create_failed");
        if (reason != nullptr) {
            *reason = m_gbmDeviceFailureReason;
        }
        return false;
    }

    m_gbmDevice = device;
    m_gbmDeviceFd = fd;
    m_gbmDevicePath = path;
    m_gbmDeviceFailureReason.clear();
    qInfo().noquote() << kLogPrefix << "gbm device initialized"
                      << "path =" << m_gbmDevicePath
                      << "backend =" << QString::fromUtf8(gbm_device_get_backend_name(m_gbmDevice));

    if (reason != nullptr) {
        reason->clear();
    }
    return true;
}

gbm_device *WaylandRenderer::gbmDevice() const {
    return m_gbmDevice;
}

bool WaylandRenderer::ensureDmabufAllocationDevice(QString *reason) {
    if (!m_hasSnapshot) {
        if (reason != nullptr) {
            *reason = QStringLiteral("snapshot_not_ready");
        }
        return false;
    }

    if (!m_dmabufAdvertised || m_linuxDmabuf == nullptr) {
        if (reason != nullptr) {
            *reason = QStringLiteral("dmabuf_not_advertised");
        }
        return false;
    }

    const QVector<GpuDeviceInfo> devices = gpuDevices();
    const QString renderPath = resolveRenderDevicePath(devices);
    const QString selectedPath = resolvePresentDevicePath(devices, renderPath);

    if (m_dmabufAllocationDevice != nullptr &&
        m_dmabufAllocationDevicePath == selectedPath) {
        if (reason != nullptr) {
            reason->clear();
        }
        return true;
    }

    if (m_dmabufAllocationDevice != nullptr &&
        m_dmabufAllocationDevicePath != selectedPath) {
        if (m_dmabufAllocationDevice != nullptr) {
            gbm_device_destroy(m_dmabufAllocationDevice);
            m_dmabufAllocationDevice = nullptr;
        }
        if (m_dmabufAllocationDeviceFd >= 0) {
            ::close(m_dmabufAllocationDeviceFd);
            m_dmabufAllocationDeviceFd = -1;
        }
        m_dmabufAllocationDeviceAttempted = false;
        m_dmabufAllocationDeviceFailureReason.clear();
        m_dmabufAllocationDevicePath.clear();
    }

    if (m_dmabufAllocationDeviceAttempted) {
        if (reason != nullptr) {
            *reason = m_dmabufAllocationDeviceFailureReason;
        }
        return false;
    }

    m_dmabufAllocationDeviceAttempted = true;

    QString path = selectedPath;
    if (path.isEmpty()) {
        path = firstExistingDrmNode(
            QStringList{QStringLiteral("renderD*"), QStringLiteral("card*")}
        );
    }
    if (path.isEmpty()) {
        m_dmabufAllocationDeviceFailureReason = QStringLiteral("drm_node_missing");
        if (reason != nullptr) {
            *reason = m_dmabufAllocationDeviceFailureReason;
        }
        return false;
    }

    const int fd = ::open(path.toUtf8().constData(), O_RDWR | O_CLOEXEC);
    if (fd < 0) {
        m_dmabufAllocationDeviceFailureReason = QStringLiteral("drm_node_open_failed");
        qWarning().noquote() << kLogPrefix
                             << "failed to open dmabuf allocation DRM node"
                             << path
                             << std::strerror(errno);
        if (reason != nullptr) {
            *reason = m_dmabufAllocationDeviceFailureReason;
        }
        return false;
    }

    gbm_device *device = gbm_create_device(fd);
    if (device == nullptr) {
        ::close(fd);
        m_dmabufAllocationDeviceFailureReason = QStringLiteral("gbm_device_create_failed");
        if (reason != nullptr) {
            *reason = m_dmabufAllocationDeviceFailureReason;
        }
        return false;
    }

    m_dmabufAllocationDevice = device;
    m_dmabufAllocationDeviceFd = fd;
    m_dmabufAllocationDevicePath = path;
    m_dmabufAllocationDeviceFailureReason.clear();
    qInfo().noquote() << kLogPrefix << "dmabuf allocation device initialized"
                      << "path =" << m_dmabufAllocationDevicePath
                      << "backend ="
                      << QString::fromUtf8(
                             gbm_device_get_backend_name(m_dmabufAllocationDevice)
                         );

    if (reason != nullptr) {
        reason->clear();
    }
    return true;
}

gbm_device *WaylandRenderer::dmabufAllocationDevice() const {
    return m_dmabufAllocationDevice;
}

WaylandRenderer::PresentBackendSelection WaylandRenderer::resolvePresentBackend(
    const QString &requested
) const {
    const QString normalizedRequested = normalizePresentBackend(requested);
    if (normalizedRequested == QLatin1String("shm")) {
        return PresentBackendSelection{
            .requested = normalizedRequested,
            .resolved = QStringLiteral("shm"),
        };
    }

    const QVector<GpuDeviceInfo> devices = gpuDevices();
    const QString compositorPath = compositorDevicePath();
    const QString renderPath = resolveRenderDevicePath(devices);
    const QString presentPath = resolvePresentDevicePath(devices, renderPath);
    const bool unsafeCrossGpuDmabuf =
        !compositorPath.isEmpty() &&
        !presentPath.isEmpty() &&
        compositorPath != presentPath &&
        gpuInfoForPath(devices, presentPath).has_value() &&
        gpuInfoForPath(devices, presentPath)->vendor == GpuVendor::Nvidia;
    const QString unsafeCrossGpuReason =
        QStringLiteral("cross_gpu_nvidia_dmabuf_unsafe");

    if (normalizedRequested == QLatin1String("dmabuf")) {
        if (!m_dmabufAdvertised || unsafeCrossGpuDmabuf) {
            return PresentBackendSelection{
                .requested = normalizedRequested,
                .resolved = QStringLiteral("shm"),
                .fallbackReason = !m_dmabufAdvertised
                    ? QStringLiteral("dmabuf_not_advertised")
                    : unsafeCrossGpuReason,
            };
        }
        return PresentBackendSelection{
            .requested = normalizedRequested,
            .resolved = QStringLiteral("dmabuf"),
        };
    }

    if (m_dmabufAdvertised && !unsafeCrossGpuDmabuf) {
        return PresentBackendSelection{
            .requested = normalizedRequested,
            .resolved = QStringLiteral("dmabuf"),
        };
    }

    return PresentBackendSelection{
        .requested = normalizedRequested,
        .resolved = QStringLiteral("shm"),
        .fallbackReason = !m_dmabufAdvertised
            ? QStringLiteral("dmabuf_not_advertised")
            : unsafeCrossGpuReason,
    };
}

bool WaylandRenderer::dmabufAdvertised() const {
    return m_dmabufAdvertised;
}

uint32_t WaylandRenderer::dmabufVersion() const {
    return m_dmabufVersion;
}

quint64 WaylandRenderer::dmabufFormatCount() const {
    return m_dmabufFormatCount;
}

quint64 WaylandRenderer::dmabufModifierCount() const {
    return m_dmabufModifierCount;
}

std::optional<dev_t> WaylandRenderer::dmabufMainDevice() const {
    return m_dmabufMainDevice;
}

QString WaylandRenderer::gbmDevicePath() const {
    return m_gbmDevicePath;
}

QString WaylandRenderer::dmabufAllocationDevicePath() const {
    return m_dmabufAllocationDevicePath;
}

bool WaylandRenderer::conservativeDmabufSyncEnabled() const {
    const QVector<GpuDeviceInfo> devices = gpuDevices();
    const auto renderGpu = gpuInfoForPath(devices, m_effectiveRenderDevicePath);
    return !m_effectivePresentDevicePath.isEmpty() &&
           !m_effectiveRenderDevicePath.isEmpty() &&
           m_effectivePresentDevicePath != m_effectiveRenderDevicePath &&
           renderGpu.has_value() &&
           renderGpu->vendor == GpuVendor::Nvidia;
}

zwlr_layer_shell_v1 *WaylandRenderer::layerShell() const {
    return m_layerShell;
}

void WaylandRenderer::flush() {
    if (m_display != nullptr) {
        wl_display_flush(m_display);
    }
}

void WaylandRenderer::rebindOutputs() {
    for (auto &entry : m_outputs) {
        entry.second->applySnapshot(m_snapshot, m_sources);
    }
}

QVector<GpuDeviceInfo> WaylandRenderer::gpuDevices() const {
    return discoverGpuDevices();
}

QString WaylandRenderer::compositorDevicePath() const {
    if (!m_dmabufMainDevice.has_value()) {
        return QString();
    }
    return drmNodePathForDevice(*m_dmabufMainDevice);
}

QString WaylandRenderer::resolveRenderDevicePath(
    const QVector<GpuDeviceInfo> &devices
) const {
    const QString compositorPath = compositorDevicePath();
    const QString policy = normalizeGpuPolicy(
        m_snapshot.renderDevicePolicy,
        QStringLiteral("same-as-compositor")
    );

    if (policy == QLatin1String("same-as-compositor") ||
        policy == QLatin1String("same-as-render")) {
        return compositorPath;
    }

    if (!m_snapshot.allowCrossGpu && !compositorPath.isEmpty()) {
        return compositorPath;
    }

    const QString selected = selectGpuDevicePath(devices, policy, compositorPath);
    if (!selected.isEmpty()) {
        return selected;
    }

    return compositorPath;
}

QString WaylandRenderer::resolvePresentDevicePath(
    const QVector<GpuDeviceInfo> &,
    const QString &renderDevicePath
) const {
    const QString compositorPath = compositorDevicePath();
    if (renderDevicePath.isEmpty()) {
        return compositorPath;
    }
    if (compositorPath.isEmpty() || compositorPath == renderDevicePath) {
        return renderDevicePath;
    }

    if (!compositorPath.isEmpty()) {
        return compositorPath;
    }

    return renderDevicePath;
}

QString WaylandRenderer::resolveDecodeDevicePath(
    const QVector<GpuDeviceInfo> &devices,
    const QString &renderDevicePath
) const {
    const QString compositorPath = compositorDevicePath();
    const QString policy = normalizeGpuPolicy(
        m_snapshot.decodeDevicePolicy,
        QStringLiteral("same-as-render")
    );

    if (policy == QLatin1String("same-as-render")) {
        return !renderDevicePath.isEmpty() ? renderDevicePath : compositorPath;
    }
    if (policy == QLatin1String("same-as-compositor")) {
        return !compositorPath.isEmpty() ? compositorPath : renderDevicePath;
    }

    if (!m_snapshot.allowCrossGpu && !renderDevicePath.isEmpty()) {
        return renderDevicePath;
    }

    const QString selected = selectGpuDevicePath(devices, policy, renderDevicePath);
    if (!selected.isEmpty()) {
        return selected;
    }

    if (!renderDevicePath.isEmpty()) {
        return renderDevicePath;
    }

    return compositorPath;
}

QStringList WaylandRenderer::resolveDecodeBackendOrder(
    const QVector<GpuDeviceInfo> &devices,
    const QString &decodeDevicePath
) const {
    const auto decodeGpu = gpuInfoForPath(devices, decodeDevicePath);
    const GpuVendor vendor = decodeGpu.has_value() ? decodeGpu->vendor : GpuVendor::Other;
    return reorderDecodeBackendsForGpu(m_snapshot.decodeBackendOrder, vendor);
}

QString WaylandRenderer::describeGpuPath(
    const QVector<GpuDeviceInfo> &devices,
    const QString &path
) const {
    if (path.isEmpty()) {
        return QStringLiteral("<none>");
    }

    const auto info = gpuInfoForPath(devices, path);
    if (!info.has_value()) {
        return path;
    }

    return QStringLiteral("%1 (%2,%3)")
        .arg(
            path,
            gpuVendorString(info->vendor),
            info->discrete ? QStringLiteral("discrete")
                           : QStringLiteral("integrated")
        );
}

void WaylandRenderer::resetGpuPipeline() {
    m_gpuCompositor.reset();
    m_gpuCompositorAttempted = false;
    m_gpuCompositorFailureReason.clear();

    if (m_gbmDevice != nullptr) {
        gbm_device_destroy(m_gbmDevice);
        m_gbmDevice = nullptr;
    }
    if (m_gbmDeviceFd >= 0) {
        ::close(m_gbmDeviceFd);
        m_gbmDeviceFd = -1;
    }

    m_gbmDeviceAttempted = false;
    m_gbmDeviceFailureReason.clear();
    m_gbmDevicePath.clear();

    if (m_dmabufAllocationDevice != nullptr) {
        gbm_device_destroy(m_dmabufAllocationDevice);
        m_dmabufAllocationDevice = nullptr;
    }
    if (m_dmabufAllocationDeviceFd >= 0) {
        ::close(m_dmabufAllocationDeviceFd);
        m_dmabufAllocationDeviceFd = -1;
    }
    m_dmabufAllocationDeviceAttempted = false;
    m_dmabufAllocationDeviceFailureReason.clear();
    m_dmabufAllocationDevicePath.clear();
}

} // namespace quicksov::wallpaper_native
