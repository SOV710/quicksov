// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "SourceSession.hpp"

#include <QImageReader>
#include <QUrl>

namespace quicksov::wallpaper::renderer {

SourceSession::SourceSession(
    const SourceSpec &spec,
    const QStringList &decodeBackendOrder,
    const QString &preferredDevicePath,
    QObject *parent
)
    : QObject(parent)
    , m_spec(spec)
    , m_decodeBackendOrder(decodeBackendOrder)
    , m_preferredDevicePath(preferredDevicePath) {
    if (m_spec.kind == QStringLiteral("video")) {
        auto *video = new VideoDecoder(this);
        video->setDebugName(QStringLiteral("source:%1").arg(m_spec.id));
        video->setMuted(m_spec.mute);
        video->setLoopEnabled(m_spec.loopEnabled);
        video->setPreferredHwdecOrder(m_decodeBackendOrder);
        video->setPreferredDevicePath(m_preferredDevicePath);
        connect(video, &VideoDecoder::renderableFrameAvailable, this, &SourceSession::updated);
        connect(video, &VideoDecoder::readyChanged, this, &SourceSession::updated);
        connect(video, &VideoDecoder::statusChanged, this, &SourceSession::updated);
        connect(video, &VideoDecoder::errorStringChanged, this, &SourceSession::updated);
        connect(video, &VideoDecoder::hwdecCurrentChanged, this, &SourceSession::updated);
        video->setSource(QUrl::fromLocalFile(m_spec.path));
        m_video = video;
    } else {
        QImageReader reader(m_spec.path);
        reader.setAutoTransform(true);
        m_image = reader.read();
        if (m_image.isNull()) {
            qWarning().noquote() << kLogPrefix << "failed to load image wallpaper"
                                 << m_spec.id << m_spec.path << reader.errorString();
        }
    }
}

const SourceSpec &SourceSession::spec() const {
    return m_spec;
}

bool SourceSession::isVideo() const {
    return m_video != nullptr;
}

bool SourceSession::ready() const {
    if (m_video != nullptr) {
        return m_video->isReady() && m_video->hasRenderableFrame();
    }
    return !m_image.isNull();
}

bool SourceSession::matches(
    const SourceSpec &spec,
    const QStringList &decodeBackendOrder,
    const QString &preferredDevicePath
) const {
    return m_spec.path == spec.path &&
           m_spec.kind == spec.kind &&
           m_spec.loopEnabled == spec.loopEnabled &&
           m_spec.mute == spec.mute &&
           m_decodeBackendOrder == decodeBackendOrder &&
           m_preferredDevicePath == preferredDevicePath;
}

SourceSession::StatsSnapshot SourceSession::statsSnapshot() const {
    StatsSnapshot stats{
        .id = m_spec.id,
        .kind = m_spec.kind,
        .status = m_image.isNull() ? QStringLiteral("empty") : QStringLiteral("ready"),
        .ready = !m_image.isNull(),
    };

    if (m_video != nullptr) {
        const auto videoStats = m_video->statsSnapshot();
        stats.status = videoStats.status;
        stats.hwdecCurrent = videoStats.hwdecCurrent;
        stats.videoSize = videoStats.videoSize;
        stats.frameSize = videoStats.frameSize;
        stats.decodedFrames = videoStats.decodedFrames;
        stats.ready = ready();
    }

    return stats;
}

void SourceSession::updateRenderHint(QObject *owner, const QSize &size, bool cpuFrameRequired) {
    if (m_video != nullptr) {
        m_video->updateRenderTargetHint(owner, size);
        m_video->setCpuFrameRequired(owner, cpuFrameRequired);
    }
}

void SourceSession::removeRenderHint(QObject *owner) {
    if (m_video != nullptr) {
        m_video->removeRenderTargetHint(owner);
    }
}

VideoDecoder::HardwareFrameSnapshot SourceSession::hardwareFrameSnapshot() const {
    if (m_video == nullptr) {
        return {};
    }
    return m_video->hardwareFrameSnapshot();
}

bool SourceSession::paint(
    QPainter &painter,
    const QSize &targetSize,
    const std::optional<CropRect> &crop,
    qreal opacity
) const {
    if (m_video != nullptr) {
        const auto frame = m_video->frameSnapshot();
        if (!frame.hasFrame || frame.image.isNull()) {
            return false;
        }
        paintImageCover(painter, frame.image, targetSize, crop, opacity);
        return true;
    }

    if (m_image.isNull()) {
        return false;
    }

    paintImageCover(painter, m_image, targetSize, crop, opacity);
    return true;
}

} // namespace quicksov::wallpaper::renderer
