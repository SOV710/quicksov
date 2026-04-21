// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "SnapshotModel.hpp"

#include "../../decoder/ffmpeg/VideoDecoder.hpp"

#include <QImage>
#include <QObject>
#include <QPointer>

namespace quicksov::wallpaper::renderer {

using VideoDecoder = quicksov::wallpaper::decoder::ffmpeg::VideoDecoder;

class SourceSession final : public QObject {
    Q_OBJECT

public:
    struct StatsSnapshot {
        QString id;
        QString kind;
        QString status;
        QString hwdecCurrent;
        QSize videoSize;
        QSize frameSize;
        quint64 decodedFrames = 0;
        bool ready = false;
    };

    explicit SourceSession(
        const SourceSpec &spec,
        const QStringList &decodeBackendOrder,
        const QString &preferredDevicePath,
        QObject *parent = nullptr
    );

    [[nodiscard]] const SourceSpec &spec() const;
    [[nodiscard]] bool isVideo() const;
    [[nodiscard]] bool ready() const;
    [[nodiscard]] bool matches(
        const SourceSpec &spec,
        const QStringList &decodeBackendOrder,
        const QString &preferredDevicePath
    ) const;
    [[nodiscard]] StatsSnapshot statsSnapshot() const;

    void updateRenderHint(QObject *owner, const QSize &size, bool cpuFrameRequired);
    void removeRenderHint(QObject *owner);
    [[nodiscard]] VideoDecoder::HardwareFrameSnapshot hardwareFrameSnapshot() const;
    bool paint(
        QPainter &painter,
        const QSize &targetSize,
        const std::optional<CropRect> &crop,
        qreal opacity
    ) const;

signals:
    void updated();

private:
    SourceSpec m_spec;
    QStringList m_decodeBackendOrder;
    QString m_preferredDevicePath;
    QImage m_image;
    QPointer<VideoDecoder> m_video;
};

} // namespace quicksov::wallpaper::renderer
