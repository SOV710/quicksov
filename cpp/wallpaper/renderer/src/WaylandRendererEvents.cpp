// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WaylandRenderer.hpp"
#include "GpuCompositor.hpp"
#include "OutputSurface.hpp"
#include "SourceSession.hpp"

#include <algorithm>

#include <QDebug>

namespace quicksov::wallpaper::renderer {

void WaylandRenderer::logTelemetry() {
    const QVector<GpuDeviceInfo> devices = gpuDevices();
    const QString compositorPath = compositorDevicePath();
    const QString renderPath = resolveRenderDevicePath(devices);
    const QString presentPath = resolvePresentDevicePath(devices, renderPath);
    const QString decodePath = resolveDecodeDevicePath(devices, renderPath);
    const QStringList decodeBackendOrder = resolveDecodeBackendOrder(devices, decodePath);

    qInfo().noquote() << kLogPrefix
                      << "telemetry"
                      << "sources =" << static_cast<int>(m_sources.size())
                      << "outputs =" << static_cast<int>(m_outputs.size())
                      << "requested_present_backend =" << m_snapshot.presentBackend
                      << "render_policy =" << m_snapshot.renderDevicePolicy
                      << "decode_policy =" << m_snapshot.decodeDevicePolicy
                      << "allow_cross_gpu =" << m_snapshot.allowCrossGpu
                      << "compositor_device =" << describeGpuPath(devices, compositorPath)
                      << "render_device =" << describeGpuPath(devices, renderPath)
                      << "present_device =" << describeGpuPath(devices, presentPath)
                      << "decode_device =" << describeGpuPath(devices, decodePath)
                      << "decode_backends =" << decodeBackendOrder.join(QLatin1Char(','))
                      << "dmabuf_advertised =" << m_dmabufAdvertised
                      << "dmabuf_version =" << m_dmabufVersion
                      << "dmabuf_formats =" << m_dmabufFormatCount
                      << "dmabuf_modifiers =" << m_dmabufModifierCount;

    constexpr double intervalSeconds = 5.0;

    for (const auto &[id, source] : m_sources) {
        const auto stats = source->statsSnapshot();
        const quint64 previous = m_prevDecodedFrames.value(id, 0);
        const quint64 delta = stats.decodedFrames - previous;
        m_prevDecodedFrames.insert(id, stats.decodedFrames);

        qInfo().noquote() << kLogPrefix
                          << "source"
                          << id
                          << "kind =" << stats.kind
                          << "ready =" << stats.ready
                          << "status =" << stats.status
                          << "hwdec ="
                          << (stats.hwdecCurrent.isEmpty()
                                  ? QStringLiteral("n/a")
                                  : stats.hwdecCurrent)
                          << "decoded_total =" << stats.decodedFrames
                          << "decoded_fps ="
                          << QString::number(
                                 static_cast<double>(delta) / intervalSeconds,
                                 'f',
                                 1
                             )
                          << "video ="
                          << QStringLiteral("%1x%2")
                                 .arg(stats.videoSize.width())
                                 .arg(stats.videoSize.height())
                          << "frame ="
                          << QStringLiteral("%1x%2")
                                 .arg(stats.frameSize.width())
                                 .arg(stats.frameSize.height());
    }

    for (const auto &[registryName, output] : m_outputs) {
        const auto stats = output->statsSnapshot();
        const QString key = stats.outputName.isEmpty()
            ? QString::number(registryName)
            : stats.outputName;
        const quint64 previousCommitted = m_prevCommittedFrames.value(key, 0);
        const quint64 previousPresented = m_prevPresentedFrames.value(key, 0);
        const quint64 previousStarved = m_prevBufferStarvedFrames.value(key, 0);
        const quint64 committedDelta = stats.committedFrames - previousCommitted;
        const quint64 presentedDelta = stats.presentedFrames - previousPresented;
        const quint64 starvedDelta = stats.bufferStarvedFrames - previousStarved;

        m_prevCommittedFrames.insert(key, stats.committedFrames);
        m_prevPresentedFrames.insert(key, stats.presentedFrames);
        m_prevBufferStarvedFrames.insert(key, stats.bufferStarvedFrames);

        qInfo().noquote() << kLogPrefix
                          << "output"
                          << (stats.outputName.isEmpty()
                                  ? QStringLiteral("<unnamed>")
                                  : stats.outputName)
                          << "source ="
                          << (stats.sourceId.isEmpty()
                                  ? QStringLiteral("<none>")
                                  : stats.sourceId)
                          << "present_backend_requested ="
                          << stats.requestedPresentBackend
                          << "present_backend_resolved ="
                          << stats.resolvedPresentBackend
                          << "present_backend_fallback ="
                          << (stats.presentBackendFallbackReason.isEmpty()
                                  ? QStringLiteral("none")
                                  : stats.presentBackendFallbackReason)
                          << "configured =" << stats.configured
                          << "logical ="
                          << QStringLiteral("%1x%2")
                                 .arg(stats.logicalSize.width())
                                 .arg(stats.logicalSize.height())
                          << "pixel ="
                          << QStringLiteral("%1x%2")
                                 .arg(stats.pixelSize.width())
                                 .arg(stats.pixelSize.height())
                          << "commit_total =" << stats.committedFrames
                          << "commit_fps ="
                          << QString::number(
                                 static_cast<double>(committedDelta) / intervalSeconds,
                                 'f',
                                 1
                             )
                          << "present_total =" << stats.presentedFrames
                          << "present_fps ="
                          << QString::number(
                                 static_cast<double>(presentedDelta) / intervalSeconds,
                                 'f',
                                 1
                             )
                          << "buffer_starved_total =" << stats.bufferStarvedFrames
                          << "buffer_starved_5s =" << starvedDelta;
    }
}

void WaylandRenderer::registryGlobal(
    void *data,
    wl_registry *registry,
    uint32_t name,
    const char *interface,
    uint32_t version
) {
    auto *self = static_cast<WaylandRenderer *>(data);
    const QByteArray iface(interface);

    if (iface == "wl_compositor") {
        self->m_compositor = static_cast<wl_compositor *>(
            wl_registry_bind(registry, name, &wl_compositor_interface, std::min(version, 4U))
        );
        return;
    }

    if (iface == "wl_shm") {
        self->m_shm = static_cast<wl_shm *>(
            wl_registry_bind(registry, name, &wl_shm_interface, 1)
        );
        return;
    }

    if (iface == "zwp_linux_dmabuf_v1") {
        self->m_linuxDmabuf = static_cast<zwp_linux_dmabuf_v1 *>(
            wl_registry_bind(
                registry,
                name,
                &zwp_linux_dmabuf_v1_interface,
                std::min(version, 4U)
            )
        );
        self->m_dmabufAdvertised = true;
        self->m_dmabufVersion = std::min(version, 4U);
        static constexpr zwp_linux_dmabuf_v1_listener dmabufListener = {
            .format = &WaylandRenderer::dmabufFormat,
            .modifier = &WaylandRenderer::dmabufModifier,
        };
        zwp_linux_dmabuf_v1_add_listener(self->m_linuxDmabuf, &dmabufListener, self);
        if (self->m_dmabufVersion >= 4) {
            self->m_defaultDmabufFeedback =
                zwp_linux_dmabuf_v1_get_default_feedback(self->m_linuxDmabuf);
            static constexpr zwp_linux_dmabuf_feedback_v1_listener feedbackListener = {
                .done = &WaylandRenderer::defaultFeedbackDone,
                .format_table = &WaylandRenderer::defaultFeedbackFormatTable,
                .main_device = &WaylandRenderer::defaultFeedbackMainDevice,
                .tranche_done = &WaylandRenderer::defaultFeedbackTrancheDone,
                .tranche_target_device = &WaylandRenderer::defaultFeedbackTrancheTargetDevice,
                .tranche_formats = &WaylandRenderer::defaultFeedbackTrancheFormats,
                .tranche_flags = &WaylandRenderer::defaultFeedbackTrancheFlags,
            };
            zwp_linux_dmabuf_feedback_v1_add_listener(
                self->m_defaultDmabufFeedback,
                &feedbackListener,
                self
            );
        }
        return;
    }

    if (iface == "zwlr_layer_shell_v1") {
        self->m_layerShell = static_cast<zwlr_layer_shell_v1 *>(
            wl_registry_bind(
                registry,
                name,
                &zwlr_layer_shell_v1_interface,
                std::min(version, 5U)
            )
        );
        return;
    }

    if (iface == "wl_output") {
        auto *output = static_cast<wl_output *>(
            wl_registry_bind(registry, name, &wl_output_interface, std::min(version, 4U))
        );
        auto entry = std::make_unique<OutputSurface>(self, name, output);
        static constexpr wl_output_listener outputListener = {
            .geometry =
                [](void *,
                   wl_output *,
                   int32_t,
                   int32_t,
                   int32_t,
                   int32_t,
                   int32_t,
                   const char *,
                   const char *,
                   int32_t) {},
            .mode = [](void *, wl_output *, uint32_t, int32_t, int32_t, int32_t) {},
            .done = [](void *, wl_output *) {},
            .scale = [](void *outputData, wl_output *, int32_t factor) {
                static_cast<OutputSurface *>(outputData)->setScale(std::max(1, factor));
            },
            .name = [](void *outputData, wl_output *, const char *nameValue) {
                static_cast<OutputSurface *>(outputData)->setName(
                    QString::fromUtf8(nameValue)
                );
            },
            .description = [](void *, wl_output *, const char *) {},
        };
        wl_output_add_listener(output, &outputListener, entry.get());
        self->m_outputs.emplace(name, std::move(entry));
        return;
    }
}

void WaylandRenderer::registryGlobalRemove(void *data, wl_registry *, uint32_t name) {
    auto *self = static_cast<WaylandRenderer *>(data);
    self->m_outputs.erase(name);
}

void WaylandRenderer::dmabufFormat(void *data, zwp_linux_dmabuf_v1 *, uint32_t) {
    auto *self = static_cast<WaylandRenderer *>(data);
    self->m_dmabufFormatCount += 1;
}

void WaylandRenderer::dmabufModifier(
    void *data,
    zwp_linux_dmabuf_v1 *,
    uint32_t,
    uint32_t,
    uint32_t
) {
    auto *self = static_cast<WaylandRenderer *>(data);
    self->m_dmabufModifierCount += 1;
}

void WaylandRenderer::defaultFeedbackDone(void *, zwp_linux_dmabuf_feedback_v1 *) {}

void WaylandRenderer::defaultFeedbackFormatTable(
    void *,
    zwp_linux_dmabuf_feedback_v1 *,
    int32_t fd,
    uint32_t
) {
    if (fd >= 0) {
        ::close(fd);
    }
}

void WaylandRenderer::defaultFeedbackMainDevice(
    void *data,
    zwp_linux_dmabuf_feedback_v1 *,
    wl_array *device
) {
    auto *self = static_cast<WaylandRenderer *>(data);
    const QString previousPath = self->compositorDevicePath();
    self->m_dmabufMainDevice = parseDeviceNumber(device);
    const QString nextPath = self->compositorDevicePath();
    if (previousPath != nextPath && !self->m_snapshot.sources.isEmpty()) {
        self->applySnapshot(self->m_snapshot);
    }
}

void WaylandRenderer::defaultFeedbackTrancheDone(void *, zwp_linux_dmabuf_feedback_v1 *) {}

void WaylandRenderer::defaultFeedbackTrancheTargetDevice(
    void *,
    zwp_linux_dmabuf_feedback_v1 *,
    wl_array *
) {}

void WaylandRenderer::defaultFeedbackTrancheFormats(
    void *,
    zwp_linux_dmabuf_feedback_v1 *,
    wl_array *
) {}

void WaylandRenderer::defaultFeedbackTrancheFlags(
    void *,
    zwp_linux_dmabuf_feedback_v1 *,
    uint32_t
) {}

void WaylandRenderer::onSourceUpdated(SourceSession *source) {
    for (auto &entry : m_outputs) {
        if (entry.second->boundSource() == source) {
            entry.second->scheduleRender();
        }
    }
}

} // namespace quicksov::wallpaper::renderer
