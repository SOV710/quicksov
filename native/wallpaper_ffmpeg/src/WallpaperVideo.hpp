// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <condition_variable>
#include <mutex>
#include <optional>
#include <thread>

#include <QHash>
#include <QImage>
#include <QMutex>
#include <QObject>
#include <QPointer>
#include <QSize>
#include <QUrl>
#include <QtQmlIntegration/qqmlintegration.h>

class QOpenGLContext;

class WallpaperVideo : public QObject {
    Q_OBJECT
    QML_NAMED_ELEMENT(WallpaperVideo)

    Q_PROPERTY(QUrl source READ source WRITE setSource NOTIFY sourceChanged FINAL)
    Q_PROPERTY(bool muted READ muted WRITE setMuted NOTIFY mutedChanged FINAL)
    Q_PROPERTY(bool loopEnabled READ loopEnabled WRITE setLoopEnabled NOTIFY loopEnabledChanged FINAL)
    Q_PROPERTY(qreal volume READ volume WRITE setVolume NOTIFY volumeChanged FINAL)
    Q_PROPERTY(QString debugName READ debugName WRITE setDebugName NOTIFY debugNameChanged FINAL)
    Q_PROPERTY(bool ready READ isReady NOTIFY readyChanged FINAL)
    Q_PROPERTY(QString status READ status NOTIFY statusChanged FINAL)
    Q_PROPERTY(QString errorString READ errorString NOTIFY errorStringChanged FINAL)
    Q_PROPERTY(QString hwdecCurrent READ hwdecCurrent NOTIFY hwdecCurrentChanged FINAL)
    Q_PROPERTY(QSize videoSize READ videoSize NOTIFY videoSizeChanged FINAL)
    Q_PROPERTY(QSize frameSize READ frameSize NOTIFY frameSizeChanged FINAL)

public:
    struct FrameSnapshot {
        QImage image;
        QSize size;
        quint64 serial = 0;
        bool hasFrame = false;
    };

    explicit WallpaperVideo(QObject *parent = nullptr);
    ~WallpaperVideo() override;

    [[nodiscard]] QUrl source() const;
    void setSource(const QUrl &source);

    [[nodiscard]] bool muted() const;
    void setMuted(bool muted);

    [[nodiscard]] bool loopEnabled() const;
    void setLoopEnabled(bool loopEnabled);

    [[nodiscard]] qreal volume() const;
    void setVolume(qreal volume);

    [[nodiscard]] QString debugName() const;
    void setDebugName(const QString &debugName);

    [[nodiscard]] bool isReady() const;
    [[nodiscard]] QString status() const;
    [[nodiscard]] QString errorString() const;
    [[nodiscard]] QString hwdecCurrent() const;
    [[nodiscard]] QSize videoSize() const;
    [[nodiscard]] QSize frameSize() const;

    [[nodiscard]] FrameSnapshot frameSnapshot() const;

    Q_INVOKABLE void ensureInitialized();
    Q_INVOKABLE void updateRenderTargetHint(QObject *item, const QSize &size);
    Q_INVOKABLE void removeRenderTargetHint(QObject *item);
    void updateShareContextHint(QOpenGLContext *context);

signals:
    void sourceChanged();
    void mutedChanged();
    void loopEnabledChanged();
    void volumeChanged();
    void debugNameChanged();
    void readyChanged();
    void statusChanged();
    void errorStringChanged();
    void hwdecCurrentChanged();
    void videoSizeChanged();
    void frameSizeChanged();
    void frameAvailable();

private:
    void restartDecoder();
    void stopDecoder();
    void decoderMain(QString localSource, quint64 generation);
    void acceptFrame(const QImage &image, const QSize &videoSize, quint64 generation);
    [[nodiscard]] QSize targetFrameSize(const QSize &videoSize) const;
    [[nodiscard]] bool shouldStop(quint64 generation) const;
    bool waitForStop(std::chrono::nanoseconds delay, quint64 generation);
    void clearFrame();
    void setReady(bool ready);
    void setStatus(const QString &status);
    void setErrorString(const QString &errorString);
    void setHwdecCurrent(const QString &hwdecCurrent);
    [[nodiscard]] QString logPrefix() const;

    mutable QMutex m_frameMutex;
    mutable QMutex m_hintMutex;
    mutable std::mutex m_threadMutex;
    std::condition_variable m_stopCv;
    std::thread m_decoderThread;
    QHash<quintptr, QSize> m_renderTargetHints;
    QUrl m_source;
    bool m_muted = true;
    bool m_loopEnabled = true;
    qreal m_volume = 100.0;
    QString m_debugName;
    bool m_ready = false;
    QString m_status = QStringLiteral("idle");
    QString m_errorString;
    QString m_hwdecCurrent;
    QImage m_frameImage;
    QSize m_videoSizeValue;
    QSize m_frameSizeValue;
    quint64 m_frameSerial = 0;
    quint64 m_decoderGeneration = 0;
    bool m_hasFrame = false;
    bool m_stopRequested = false;
};
