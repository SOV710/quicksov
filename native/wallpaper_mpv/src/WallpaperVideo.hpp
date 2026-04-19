// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QHash>
#include <QMutex>
#include <QOffscreenSurface>
#include <QObject>
#include <QOpenGLContext>
#include <QPointer>
#include <QQuickWindow>
#include <QSize>
#include <QTimer>
#include <QUrl>
#include <QtQmlIntegration/qqmlintegration.h>

#include "MpvCore.hpp"
#include "WallpaperSharedFrame.hpp"

class QJSEngine;
class QQmlEngine;

class WallpaperVideo : public QObject {
    Q_OBJECT
    QML_NAMED_ELEMENT(WallpaperVideo)
    QML_SINGLETON

    Q_PROPERTY(QUrl source READ source WRITE setSource NOTIFY sourceChanged FINAL)
    Q_PROPERTY(bool muted READ muted WRITE setMuted NOTIFY mutedChanged FINAL)
    Q_PROPERTY(qreal volume READ volume WRITE setVolume NOTIFY volumeChanged FINAL)
    Q_PROPERTY(bool ready READ isReady NOTIFY readyChanged FINAL)
    Q_PROPERTY(QString status READ status NOTIFY statusChanged FINAL)
    Q_PROPERTY(QString errorString READ errorString NOTIFY errorStringChanged FINAL)
    Q_PROPERTY(QSize videoSize READ videoSize NOTIFY videoSizeChanged FINAL)
    Q_PROPERTY(QSize frameSize READ frameSize NOTIFY frameSizeChanged FINAL)

public:
    struct FrameSnapshot {
        GLuint textureId = 0;
        QSize size;
        quint64 serial = 0;
        bool hasFrame = false;
    };

    static WallpaperVideo *create(QQmlEngine *engine, QJSEngine *scriptEngine);

    explicit WallpaperVideo(QObject *parent = nullptr);
    ~WallpaperVideo() override;

    [[nodiscard]] QUrl source() const;
    void setSource(const QUrl &source);

    [[nodiscard]] bool muted() const;
    void setMuted(bool muted);

    [[nodiscard]] qreal volume() const;
    void setVolume(qreal volume);

    [[nodiscard]] bool isReady() const;
    [[nodiscard]] QString status() const;
    [[nodiscard]] QString errorString() const;
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
    void volumeChanged();
    void readyChanged();
    void statusChanged();
    void errorStringChanged();
    void videoSizeChanged();
    void frameSizeChanged();
    void frameAvailable();

private slots:
    void ensureGraphicsReady();
    void drainEvents();
    void scheduleRender();
    void renderFrame();

private:
    static void onWakeup(void *ctx);
    static void onRenderUpdate(void *ctx);

    bool ensureMpvCore();
    void loadCurrentSource();
    void applyAudioState();
    void updateVideoSize();
    QSize targetFrameSize() const;
    void setReady(bool ready);
    void setStatus(const QString &status);
    void setErrorString(const QString &errorString);
    void clearVideoSize();

    mutable QMutex m_frameMutex;
    MpvCore m_mpv;
    WallpaperSharedFrame m_frame;
    QOffscreenSurface *m_offscreenSurface = nullptr;
    QOpenGLContext *m_offscreenContext = nullptr;
    QPointer<QOpenGLContext> m_shareContextHint;
    QTimer m_initRetryTimer;
    QUrl m_source;
    bool m_muted = true;
    qreal m_volume = 100.0;
    bool m_ready = false;
    QString m_status = QStringLiteral("idle");
    QString m_errorString;
    QSize m_videoSizeValue;
    QSize m_frameSizeValue;
    bool m_renderScheduled = false;
    bool m_forceRender = false;
    bool m_hasFrame = false;
    bool m_loggedFirstFrame = false;
    bool m_loggedWaitingForShareContext = false;
    quint64 m_frameSerial = 0;
    GLuint m_textureId = 0;
    qint64 m_observedDwidth = 0;
    qint64 m_observedDheight = 0;
    QHash<quintptr, QSize> m_renderTargetHints;
};
