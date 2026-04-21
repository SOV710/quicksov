// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "OutputSurface.hpp"
#include "GpuCompositor.hpp"
#include "WaylandRenderer.hpp"
#include "SourceSession.hpp"

#include <algorithm>
#include <cstring>

#include <fcntl.h>
#include <linux/memfd.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

#include <QDebug>

namespace quicksov::wallpaper::renderer {

namespace {

std::optional<dev_t> deviceNumberForPath(const QString &path) {
    struct stat st {};
    if (::stat(path.toUtf8().constData(), &st) != 0) {
        return std::nullopt;
    }
    if (!S_ISCHR(st.st_mode)) {
        return std::nullopt;
    }
    return st.st_rdev;
}

} // namespace

void OutputSurface::ensureShmBuffers() {
    if (m_pixelSize.isEmpty() || m_renderer->shm() == nullptr) {
        return;
    }

    const int expectedWidth = m_pixelSize.width();
    const int expectedHeight = m_pixelSize.height();
    for (const auto &buffer : m_shmBuffers) {
        if (buffer.buffer == nullptr ||
            buffer.width != expectedWidth ||
            buffer.height != expectedHeight) {
            destroyShmBuffers();
            break;
        }
    }

    for (auto &buffer : m_shmBuffers) {
        if (buffer.buffer != nullptr) {
            continue;
        }

        buffer.width = expectedWidth;
        buffer.height = expectedHeight;
        buffer.stride = expectedWidth * 4;
        buffer.bytes = static_cast<size_t>(buffer.stride) * expectedHeight;
        buffer.owner = this;

        const int fd = ::memfd_create("qsov-wallpaper-buffer", MFD_CLOEXEC);
        if (fd < 0) {
            qWarning().noquote() << kLogPrefix << "memfd_create failed";
            destroyShmBuffers();
            return;
        }

        if (::ftruncate(fd, static_cast<off_t>(buffer.bytes)) < 0) {
            qWarning().noquote() << kLogPrefix << "ftruncate failed";
            ::close(fd);
            destroyShmBuffers();
            return;
        }

        buffer.data = ::mmap(nullptr, buffer.bytes, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
        if (buffer.data == MAP_FAILED) {
            qWarning().noquote() << kLogPrefix << "mmap failed";
            buffer.data = nullptr;
            ::close(fd);
            destroyShmBuffers();
            return;
        }

        wl_shm_pool *pool = wl_shm_create_pool(
            m_renderer->shm(),
            fd,
            static_cast<int>(buffer.bytes)
        );
        buffer.buffer = wl_shm_pool_create_buffer(
            pool,
            0,
            buffer.width,
            buffer.height,
            buffer.stride,
            WL_SHM_FORMAT_ARGB8888
        );
        wl_shm_pool_destroy(pool);
        ::close(fd);

        static constexpr wl_buffer_listener bufferListener = {
            .release = &OutputSurface::shmBufferReleased,
        };
        wl_buffer_add_listener(buffer.buffer, &bufferListener, &buffer);
    }
}

bool OutputSurface::supportsDmabufModifier(uint32_t format, uint64_t modifier) const {
    return std::any_of(
        m_surfaceDmabufFormats.cbegin(),
        m_surfaceDmabufFormats.cend(),
        [format, modifier](const DmabufFormatModifier &entry) {
            return entry.format == format && entry.modifier == modifier;
        }
    );
}

std::vector<uint64_t> OutputSurface::supportedDmabufModifiers(uint32_t format) const {
    std::vector<uint64_t> modifiers;
    for (const auto &entry : m_surfaceDmabufFormats) {
        if (entry.format != format) {
            continue;
        }
        if (std::find(modifiers.cbegin(), modifiers.cend(), entry.modifier) ==
            modifiers.cend()) {
            modifiers.push_back(entry.modifier);
        }
    }

    std::sort(modifiers.begin(), modifiers.end(), [](uint64_t left, uint64_t right) {
        auto rank = [](uint64_t modifier) {
            if (modifier == DRM_FORMAT_MOD_LINEAR) {
                return 0;
            }
            if (modifier == DRM_FORMAT_MOD_INVALID) {
                return 1;
            }
            return 2;
        };
        if (rank(left) != rank(right)) {
            return rank(left) < rank(right);
        }
        return left < right;
    });

    return modifiers;
}

std::vector<uint64_t> OutputSurface::supportedDmabufModifiersForDevice(
    uint32_t format,
    dev_t targetDevice
) const {
    std::vector<uint64_t> modifiers;
    for (const auto &tranche : m_surfaceDmabufTranches) {
        if (!tranche.targetDevice.has_value() || *tranche.targetDevice != targetDevice) {
            continue;
        }

        for (const auto &entry : tranche.formats) {
            if (entry.format != format) {
                continue;
            }
            if (std::find(modifiers.cbegin(), modifiers.cend(), entry.modifier) ==
                modifiers.cend()) {
                modifiers.push_back(entry.modifier);
            }
        }
    }

    std::sort(modifiers.begin(), modifiers.end(), [](uint64_t left, uint64_t right) {
        auto rank = [](uint64_t modifier) {
            if (modifier == DRM_FORMAT_MOD_LINEAR) {
                return 0;
            }
            if (modifier == DRM_FORMAT_MOD_INVALID) {
                return 1;
            }
            return 2;
        };
        if (rank(left) != rank(right)) {
            return rank(left) < rank(right);
        }
        return left < right;
    });

    return modifiers;
}

std::vector<uint64_t> OutputSurface::dmabufModifierCandidates(uint32_t format) const {
    auto appendUnique = [](std::vector<uint64_t> &dst, const std::vector<uint64_t> &src) {
        for (const uint64_t value : src) {
            if (std::find(dst.cbegin(), dst.cend(), value) == dst.cend()) {
                dst.push_back(value);
            }
        }
    };

    std::vector<uint64_t> modifiers;
    const QString allocationDevicePath = m_renderer->dmabufAllocationDevicePath();
    const auto allocationDevice = deviceNumberForPath(allocationDevicePath);

    if (allocationDevicePath.isEmpty()) {
        modifiers = supportedDmabufModifiers(format);
    } else if (allocationDevice.has_value()) {
        modifiers = supportedDmabufModifiersForDevice(format, *allocationDevice);
        appendUnique(modifiers, supportedDmabufModifiers(format));
    } else {
        modifiers = supportedDmabufModifiers(format);
    }

    const bool crossGpu =
        m_renderer->dmabufMainDevice().has_value() &&
        allocationDevice.has_value() &&
        *m_renderer->dmabufMainDevice() != *allocationDevice;
    const auto allocationGpu =
        gpuInfoForPath(discoverGpuDevices(), allocationDevicePath);
    const bool allocateOnNvidia =
        allocationGpu.has_value() && allocationGpu->vendor == GpuVendor::Nvidia;

    if (crossGpu || allocateOnNvidia) {
        if (std::find(modifiers.cbegin(), modifiers.cend(), DRM_FORMAT_MOD_LINEAR) ==
            modifiers.cend()) {
            modifiers.push_back(DRM_FORMAT_MOD_LINEAR);
        }
        if (std::find(modifiers.cbegin(), modifiers.cend(), DRM_FORMAT_MOD_INVALID) ==
            modifiers.cend()) {
            modifiers.push_back(DRM_FORMAT_MOD_INVALID);
        }
    } else if (modifiers.empty()) {
        modifiers = {DRM_FORMAT_MOD_LINEAR, DRM_FORMAT_MOD_INVALID};
    }

    return modifiers;
}

bool OutputSurface::createDmabufBuffer(DmaBuffer &buffer) {
    QString reason;
    if (!m_renderer->ensureDmabufAllocationDevice(&reason) ||
        m_renderer->dmabufAllocationDevice() == nullptr) {
        m_presentBackendFallbackReason = reason.isEmpty()
            ? QStringLiteral("gbm_device_unavailable")
            : reason;
        return false;
    }

    if (m_renderer->dmabufVersion() >= 4 && !m_dmabufFeedbackReady) {
        m_presentBackendFallbackReason = QStringLiteral("dmabuf_feedback_pending");
        return false;
    }

    const auto allocationGpu = gpuInfoForPath(
        discoverGpuDevices(),
        m_renderer->dmabufAllocationDevicePath()
    );
    const bool allocateOnNvidia =
        allocationGpu.has_value() && allocationGpu->vendor == GpuVendor::Nvidia;

    std::vector<uint32_t> usageCandidates = allocateOnNvidia
        ? std::vector<uint32_t>{
            GBM_BO_USE_RENDERING | GBM_BO_USE_SCANOUT,
            GBM_BO_USE_RENDERING | GBM_BO_USE_SCANOUT | GBM_BO_USE_FRONT_RENDERING,
            GBM_BO_USE_RENDERING,
            GBM_BO_USE_SCANOUT,
        }
        : std::vector<uint32_t>{
            GBM_BO_USE_RENDERING,
            GBM_BO_USE_RENDERING | GBM_BO_USE_SCANOUT,
            GBM_BO_USE_SCANOUT,
        };

    auto appendUsageCandidate = [&](uint32_t usage) {
        if (std::find(usageCandidates.cbegin(), usageCandidates.cend(), usage) ==
            usageCandidates.cend()) {
            usageCandidates.push_back(usage);
        }
    };
    appendUsageCandidate(GBM_BO_USE_RENDERING | GBM_BO_USE_LINEAR);
    appendUsageCandidate(GBM_BO_USE_RENDERING | GBM_BO_USE_SCANOUT | GBM_BO_USE_LINEAR);
    appendUsageCandidate(GBM_BO_USE_SCANOUT | GBM_BO_USE_LINEAR);

    gbm_bo *bo = nullptr;
    uint64_t protocolModifier = DRM_FORMAT_MOD_INVALID;
    uint32_t selectedFormat = kDmabufDrmFormat;
    QStringList attemptLabels;
    const bool preferOpaqueFormat = allocateOnNvidia;
    const std::vector<uint32_t> formatCandidates = preferOpaqueFormat
        ? std::vector<uint32_t>{DRM_FORMAT_XRGB8888, DRM_FORMAT_ARGB8888}
        : std::vector<uint32_t>{DRM_FORMAT_ARGB8888, DRM_FORMAT_XRGB8888};

    auto usageLabelFor = [](uint32_t usage) {
        QStringList parts;
        if ((usage & GBM_BO_USE_RENDERING) != 0) {
            parts.push_back(QStringLiteral("rendering"));
        }
#ifdef GBM_BO_USE_TEXTURING
        if ((usage & GBM_BO_USE_TEXTURING) != 0) {
            parts.push_back(QStringLiteral("texturing"));
        }
#endif
        if ((usage & GBM_BO_USE_SCANOUT) != 0) {
            parts.push_back(QStringLiteral("scanout"));
        }
        if ((usage & GBM_BO_USE_FRONT_RENDERING) != 0) {
            parts.push_back(QStringLiteral("front"));
        }
        if ((usage & GBM_BO_USE_LINEAR) != 0) {
            parts.push_back(QStringLiteral("linear"));
        }
        return parts.join(QLatin1Char('+'));
    };

    for (const uint32_t drmFormat : formatCandidates) {
        std::vector<uint64_t> modifiers = dmabufModifierCandidates(drmFormat);
        if (m_renderer->dmabufVersion() >= 4 && modifiers.empty()) {
            continue;
        }
        if (modifiers.empty()) {
            modifiers = {DRM_FORMAT_MOD_LINEAR, DRM_FORMAT_MOD_INVALID};
        }

        for (const uint64_t advertisedModifier : modifiers) {
            for (const uint32_t usage : usageCandidates) {
                if ((usage & GBM_BO_USE_LINEAR) != 0 &&
                    advertisedModifier != DRM_FORMAT_MOD_LINEAR &&
                    advertisedModifier != DRM_FORMAT_MOD_INVALID) {
                    continue;
                }

                const bool modifierWasAdvertised =
                    advertisedModifier == DRM_FORMAT_MOD_INVALID ||
                    supportsDmabufModifier(drmFormat, advertisedModifier);
                const QString usageLabel = usageLabelFor(usage);

                if (advertisedModifier == DRM_FORMAT_MOD_INVALID) {
                    bo = gbm_bo_create(
                        m_renderer->dmabufAllocationDevice(),
                        static_cast<uint32_t>(m_pixelSize.width()),
                        static_cast<uint32_t>(m_pixelSize.height()),
                        drmFormat,
                        usage
                    );
                    protocolModifier = DRM_FORMAT_MOD_INVALID;
                } else {
                    const uint64_t modifier = advertisedModifier;
                    bo = gbm_bo_create_with_modifiers2(
                        m_renderer->dmabufAllocationDevice(),
                        static_cast<uint32_t>(m_pixelSize.width()),
                        static_cast<uint32_t>(m_pixelSize.height()),
                        drmFormat,
                        &modifier,
                        1,
                        usage
                    );
                    protocolModifier = modifier;
                }

                if (bo == nullptr) {
                    attemptLabels.push_back(
                        QStringLiteral("%1/%2@%3")
                            .arg(
                                drmFormatString(drmFormat),
                                dmabufModifierString(advertisedModifier),
                                usageLabel
                            )
                    );
                    continue;
                }

                if (gbm_bo_get_plane_count(bo) != 1) {
                    gbm_bo_destroy(bo);
                    bo = nullptr;
                    continue;
                }

                const uint64_t actualModifier = gbm_bo_get_modifier(bo);
                if (actualModifier != DRM_FORMAT_MOD_INVALID) {
                    protocolModifier = actualModifier;
                }

                if (protocolModifier != DRM_FORMAT_MOD_INVALID) {
                    if (modifierWasAdvertised &&
                        !supportsDmabufModifier(drmFormat, actualModifier)) {
                        gbm_bo_destroy(bo);
                        bo = nullptr;
                        continue;
                    }
                }

                selectedFormat = drmFormat;
                break;
            }

            if (bo != nullptr) {
                break;
            }
        }

        if (bo != nullptr) {
            break;
        }
    }

    if (bo == nullptr) {
        m_presentBackendFallbackReason = QStringLiteral("gbm_bo_create_failed");
        const QString attempts = attemptLabels.join(QStringLiteral(", "));
        const QString signature = QStringLiteral("%1|%2|%3")
            .arg(m_outputName, m_renderer->dmabufAllocationDevicePath(), attempts);
        if (signature != m_lastDmabufAllocFailureSignature) {
            m_lastDmabufAllocFailureSignature = signature;
            m_dmabufAllocFailureRepeats = 0;
        }
        if (m_dmabufAllocFailureRepeats == 0 || m_dmabufAllocFailureRepeats % 120 == 0) {
            qWarning().noquote() << kLogPrefix
                                 << "dmabuf GBM allocation failed"
                                 << m_outputName
                                 << "device =" << m_renderer->dmabufAllocationDevicePath()
                                 << "repeats =" << m_dmabufAllocFailureRepeats
                                 << "attempts =" << attempts;
        }
        m_dmabufAllocFailureRepeats += 1;
        return false;
    }

    m_lastDmabufAllocFailureSignature.clear();
    m_dmabufAllocFailureRepeats = 0;

    uint32_t planeStride = gbm_bo_get_stride_for_plane(bo, 0);
    if (planeStride == 0) {
        planeStride = gbm_bo_get_stride(bo);
    }
    const uint32_t planeOffset = gbm_bo_get_offset(bo, 0);
    int fd = gbm_bo_get_fd_for_plane(bo, 0);
    if (fd < 0) {
        fd = gbm_bo_get_fd(bo);
    }
    if (fd < 0) {
        gbm_bo_destroy(bo);
        m_presentBackendFallbackReason = QStringLiteral("gbm_bo_export_failed");
        return false;
    }

    int gpuImportFd = gbm_bo_get_fd_for_plane(bo, 0);
    if (gpuImportFd < 0) {
        gpuImportFd = gbm_bo_get_fd(bo);
    }
    if (gpuImportFd < 0) {
        ::close(fd);
        gbm_bo_destroy(bo);
        m_presentBackendFallbackReason = QStringLiteral("gbm_bo_export_failed");
        return false;
    }

    zwp_linux_buffer_params_v1 *params =
        zwp_linux_dmabuf_v1_create_params(m_renderer->linuxDmabuf());
    if (params == nullptr) {
        ::close(fd);
        gbm_bo_destroy(bo);
        m_presentBackendFallbackReason = QStringLiteral("dmabuf_params_create_failed");
        return false;
    }
    zwp_linux_buffer_params_v1_add(
        params,
        fd,
        0,
        planeOffset,
        planeStride,
        modifierHi(protocolModifier),
        modifierLo(protocolModifier)
    );
    ::close(fd);

    buffer.owner = this;
    buffer.params = params;
    buffer.bo = bo;
    buffer.gpuImportFd = gpuImportFd;
    buffer.data = nullptr;
    buffer.mapData = nullptr;
    buffer.width = m_pixelSize.width();
    buffer.height = m_pixelSize.height();
    buffer.stride = static_cast<int>(planeStride);
    buffer.offset = static_cast<int>(planeOffset);
    buffer.format = selectedFormat;
    buffer.modifier = protocolModifier;
    buffer.pending = false;
    buffer.busy = false;

    if (m_renderer->dmabufVersion() >= 4) {
        wl_buffer *wlBuffer = zwp_linux_buffer_params_v1_create_immed(
            params,
            m_pixelSize.width(),
            m_pixelSize.height(),
            selectedFormat,
            0
        );
        zwp_linux_buffer_params_v1_destroy(params);
        buffer.params = nullptr;
        if (wlBuffer == nullptr) {
            releaseDmabufBuffer(buffer);
            m_presentBackendFallbackReason = QStringLiteral("dmabuf_create_immed_failed");
            return false;
        }

        static constexpr wl_buffer_listener bufferListener = {
            .release = &OutputSurface::dmabufBufferReleased,
        };
        buffer.buffer = wlBuffer;
        wl_buffer_add_listener(buffer.buffer, &bufferListener, &buffer);
    } else {
        static constexpr zwp_linux_buffer_params_v1_listener paramsListener = {
            .created = &OutputSurface::dmabufParamsCreated,
            .failed = &OutputSurface::dmabufParamsFailed,
        };
        zwp_linux_buffer_params_v1_add_listener(params, &paramsListener, &buffer);
        zwp_linux_buffer_params_v1_create(
            params,
            m_pixelSize.width(),
            m_pixelSize.height(),
            selectedFormat,
            0
        );
        buffer.pending = true;
    }

    flush();

    qInfo().noquote() << kLogPrefix
                      << "dmabuf buffer requested"
                      << m_outputName
                      << drmFormatString(buffer.format)
                      << dmabufModifierString(buffer.modifier);
    return true;
}

bool OutputSurface::ensureDmabufBuffers() {
    if (m_targetPresentBackend == QLatin1String("shm") ||
        m_pixelSize.isEmpty() ||
        m_renderer->linuxDmabuf() == nullptr ||
        m_dmabufDisabled) {
        return false;
    }

    const int expectedWidth = m_pixelSize.width();
    const int expectedHeight = m_pixelSize.height();
    for (const auto &buffer : m_dmabufBuffers) {
        if ((buffer.buffer != nullptr || buffer.pending) &&
            (buffer.width != expectedWidth || buffer.height != expectedHeight)) {
            destroyDmabufBuffers();
            break;
        }
    }

    bool anyReady = false;
    for (auto &buffer : m_dmabufBuffers) {
        if (buffer.buffer != nullptr) {
            anyReady = true;
            continue;
        }
        if (buffer.pending) {
            continue;
        }
        if (!createDmabufBuffer(buffer)) {
            return anyReady;
        }
    }

    return anyReady;
}

void OutputSurface::releaseDmabufBuffer(DmaBuffer &buffer) {
    if (buffer.buffer != nullptr) {
        wl_buffer_destroy(buffer.buffer);
        buffer.buffer = nullptr;
    }
    if (buffer.params != nullptr) {
        zwp_linux_buffer_params_v1_destroy(buffer.params);
        buffer.params = nullptr;
    }
    if (buffer.bo != nullptr && buffer.mapData != nullptr) {
        gbm_bo_unmap(buffer.bo, buffer.mapData);
        buffer.mapData = nullptr;
    }
    if (buffer.bo != nullptr) {
        gbm_bo_destroy(buffer.bo);
        buffer.bo = nullptr;
    }
    if (buffer.gpuImportFd >= 0) {
        if (m_renderer->gpuCompositor() != nullptr) {
            m_renderer->gpuCompositor()->releaseTarget(reinterpret_cast<quintptr>(&buffer));
        }
        ::close(buffer.gpuImportFd);
        buffer.gpuImportFd = -1;
    }
    buffer.data = nullptr;
    buffer.width = 0;
    buffer.height = 0;
    buffer.stride = 0;
    buffer.modifier = DRM_FORMAT_MOD_INVALID;
    buffer.busy = false;
    buffer.pending = false;
}

void OutputSurface::destroyShmBuffers() {
    for (auto &buffer : m_shmBuffers) {
        if (buffer.buffer != nullptr) {
            wl_buffer_destroy(buffer.buffer);
            buffer.buffer = nullptr;
        }
        if (buffer.data != nullptr) {
            ::munmap(buffer.data, buffer.bytes);
            buffer.data = nullptr;
        }
        buffer.bytes = 0;
        buffer.width = 0;
        buffer.height = 0;
        buffer.stride = 0;
        buffer.busy = false;
    }
}

void OutputSurface::destroyDmabufBuffers() {
    for (auto &buffer : m_dmabufBuffers) {
        releaseDmabufBuffer(buffer);
    }
}

void OutputSurface::destroyBuffers() {
    m_lastPresentedIndex = -1;
    m_lastPresentedBackend = QStringLiteral("shm");
    destroyDmabufBuffers();
    destroyShmBuffers();
}

OutputSurface::ShmBuffer *OutputSurface::nextFreeShmBuffer() {
    for (auto &buffer : m_shmBuffers) {
        if (!buffer.busy && buffer.buffer != nullptr) {
            return &buffer;
        }
    }
    return nullptr;
}

OutputSurface::DmaBuffer *OutputSurface::nextFreeDmabufBuffer() {
    for (auto &buffer : m_dmabufBuffers) {
        if (!buffer.busy && !buffer.pending && buffer.buffer != nullptr) {
            return &buffer;
        }
    }
    return nullptr;
}

void OutputSurface::disableDmabuf(const QString &reason) {
    m_dmabufDisabled = true;
    m_presentBackendFallbackReason = reason;
    m_activePresentBackend = QStringLiteral("shm");
    destroyDmabufBuffers();
}

void OutputSurface::dmabufParamsCreated(
    void *data,
    zwp_linux_buffer_params_v1 *params,
    wl_buffer *buffer
) {
    auto *dmabuf = static_cast<DmaBuffer *>(data);
    dmabuf->params = nullptr;
    dmabuf->buffer = buffer;
    dmabuf->pending = false;
    zwp_linux_buffer_params_v1_destroy(params);

    static constexpr wl_buffer_listener bufferListener = {
        .release = &OutputSurface::dmabufBufferReleased,
    };
    wl_buffer_add_listener(buffer, &bufferListener, dmabuf);

    if (dmabuf->owner != nullptr) {
        dmabuf->owner->onDmabufBufferCreated();
    }
}

void OutputSurface::dmabufParamsFailed(void *data, zwp_linux_buffer_params_v1 *params) {
    auto *dmabuf = static_cast<DmaBuffer *>(data);
    dmabuf->params = nullptr;
    dmabuf->pending = false;
    zwp_linux_buffer_params_v1_destroy(params);

    if (dmabuf->owner != nullptr) {
        dmabuf->owner->onDmabufBufferFailed(dmabuf);
    }
}

void OutputSurface::dmabufFeedbackDone(void *data, zwp_linux_dmabuf_feedback_v1 *) {
    auto *self = static_cast<OutputSurface *>(data);
    self->m_dmabufFeedbackReady = true;
    self->scheduleRender();
}

void OutputSurface::dmabufFeedbackFormatTable(
    void *data,
    zwp_linux_dmabuf_feedback_v1 *,
    int32_t fd,
    uint32_t size
) {
    auto *self = static_cast<OutputSurface *>(data);
    self->m_dmabufFormatTable.clear();
    self->m_surfaceDmabufFormats.clear();
    self->m_surfaceDmabufTranches.clear();
    self->m_pendingDmabufTranche = DmabufTranche{};

    if (fd < 0 || size == 0) {
        if (fd >= 0) {
            ::close(fd);
        }
        return;
    }

    void *mapped = ::mmap(nullptr, size, PROT_READ, MAP_PRIVATE, fd, 0);
    if (mapped != MAP_FAILED) {
        self->m_dmabufFormatTable =
            QByteArray(static_cast<const char *>(mapped), static_cast<int>(size));
        ::munmap(mapped, size);
    }
    ::close(fd);
}

void OutputSurface::dmabufFeedbackMainDevice(
    void *,
    zwp_linux_dmabuf_feedback_v1 *,
    wl_array *
) {}

void OutputSurface::dmabufFeedbackTrancheDone(
    void *data,
    zwp_linux_dmabuf_feedback_v1 *
) {
    auto *self = static_cast<OutputSurface *>(data);
    if (!self->m_pendingDmabufTranche.formats.empty()) {
        self->m_surfaceDmabufTranches.push_back(std::move(self->m_pendingDmabufTranche));
    }
    self->m_pendingDmabufTranche = DmabufTranche{};
}

void OutputSurface::dmabufFeedbackTrancheTargetDevice(
    void *data,
    zwp_linux_dmabuf_feedback_v1 *,
    wl_array *device
) {
    auto *self = static_cast<OutputSurface *>(data);
    self->m_pendingDmabufTranche.targetDevice = parseDeviceNumber(device);
}

void OutputSurface::dmabufFeedbackTrancheFormats(
    void *data,
    zwp_linux_dmabuf_feedback_v1 *,
    wl_array *indices
) {
    auto *self = static_cast<OutputSurface *>(data);
    if (indices == nullptr || indices->data == nullptr || self->m_dmabufFormatTable.isEmpty()) {
        return;
    }

    const auto *table =
        reinterpret_cast<const unsigned char *>(self->m_dmabufFormatTable.constData());
    const size_t tableSize = static_cast<size_t>(self->m_dmabufFormatTable.size());
    const auto *begin = static_cast<const uint16_t *>(indices->data);
    const auto count = indices->size / sizeof(uint16_t);

    for (size_t i = 0; i < count; ++i) {
        const size_t offset = static_cast<size_t>(begin[i]) * 16;
        if (offset + 16 > tableSize) {
            continue;
        }

        uint32_t format = 0;
        uint64_t modifier = DRM_FORMAT_MOD_INVALID;
        std::memcpy(&format, table + offset, sizeof(uint32_t));
        std::memcpy(&modifier, table + offset + 8, sizeof(uint64_t));

        const DmabufFormatModifier entry{
            .format = format,
            .modifier = modifier,
        };
        if (std::find(
                self->m_surfaceDmabufFormats.cbegin(),
                self->m_surfaceDmabufFormats.cend(),
                entry
            ) == self->m_surfaceDmabufFormats.cend()) {
            self->m_surfaceDmabufFormats.push_back(entry);
        }
        if (std::find(
                self->m_pendingDmabufTranche.formats.cbegin(),
                self->m_pendingDmabufTranche.formats.cend(),
                entry
            ) == self->m_pendingDmabufTranche.formats.cend()) {
            self->m_pendingDmabufTranche.formats.push_back(entry);
        }
    }
}

void OutputSurface::dmabufFeedbackTrancheFlags(
    void *data,
    zwp_linux_dmabuf_feedback_v1 *,
    uint32_t flags
) {
    auto *self = static_cast<OutputSurface *>(data);
    self->m_pendingDmabufTranche.flags = flags;
}

void OutputSurface::onDmabufBufferCreated() {
    scheduleRender();
}

void OutputSurface::onDmabufBufferFailed(DmaBuffer *buffer) {
    if (buffer == nullptr) {
        return;
    }

    qWarning().noquote() << kLogPrefix
                         << "dmabuf buffer import failed"
                         << m_outputName
                         << drmFormatString(buffer->format)
                         << dmabufModifierString(buffer->modifier);
    releaseDmabufBuffer(*buffer);
    disableDmabuf(QStringLiteral("dmabuf_import_failed"));
    scheduleRender();
}

} // namespace quicksov::wallpaper::renderer
