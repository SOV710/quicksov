// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperVideo.hpp"

#include <algorithm>
#include <cmath>

#include <QGuiApplication>
#include <QDebug>
#include <QOpenGLFunctions>
#include <QQuickWindow>
#include <QtGui/qguiapplication_platform.h>

namespace {

QString mpvEventErrorString(int code) {
    const char *message = mpv_error_string(code);
    if (message == nullptr || *message == '\0') {
        return QStringLiteral("unknown mpv error");
    }
    return QString::fromUtf8(message);
}

QSize clampSize(const QSize &size) {
    return QSize(std::max(1, size.width()), std::max(1, size.height()));
}

} // namespace

WallpaperVideo::WallpaperVideo(QObject *parent)
    : QObject(parent) {
    m_initRetryTimer.setInterval(250);
    m_initRetryTimer.setSingleShot(true);
    connect(&m_initRetryTimer, &QTimer::timeout, this, &WallpaperVideo::ensureGraphicsReady);

    ensureMpvCore();
    qInfo().noquote() << logPrefix() << "controller created";
}

WallpaperVideo::~WallpaperVideo() {
    if (m_offscreenContext != nullptr && m_offscreenSurface != nullptr) {
        if (m_offscreenContext->makeCurrent(m_offscreenSurface)) {
            m_frame.reset();
            m_mpv.destroyRenderContext();
            m_offscreenContext->doneCurrent();
        }
    }
}

QUrl WallpaperVideo::source() const {
    return m_source;
}

void WallpaperVideo::setSource(const QUrl &source) {
    if (m_source == source) {
        return;
    }

    m_source = source;
    clearVideoSize();
    setErrorString(QString());
    setReady(false);
    m_forceRender = true;
    emit sourceChanged();
    qInfo().noquote() << logPrefix() << "source set:"
                      << (m_source.isEmpty() ? QStringLiteral("<empty>") : m_source.toString());

    if (m_source.isEmpty()) {
        setStatus(QStringLiteral("idle"));
        if (m_mpv.isInitialized()) {
            QString error;
            (void)m_mpv.command({QStringLiteral("stop")}, &error);
        }
        return;
    }

    setStatus(QStringLiteral("loading"));
    if (m_offscreenContext != nullptr && m_mpv.renderContext() != nullptr) {
        loadCurrentSource();
    } else {
        ensureInitialized();
    }
}

bool WallpaperVideo::muted() const {
    return m_muted;
}

void WallpaperVideo::setMuted(bool muted) {
    if (m_muted == muted) {
        return;
    }

    m_muted = muted;
    emit mutedChanged();
    qInfo().noquote() << logPrefix() << "muted =" << m_muted;
    applyAudioState();
}

qreal WallpaperVideo::volume() const {
    return m_volume;
}

void WallpaperVideo::setVolume(qreal volume) {
    const qreal clamped = std::clamp(volume, 0.0, 100.0);
    if (qFuzzyCompare(m_volume, clamped)) {
        return;
    }

    m_volume = clamped;
    emit volumeChanged();
    qInfo().noquote() << logPrefix() << "volume =" << m_volume;
    applyAudioState();
}

QString WallpaperVideo::debugName() const {
    return m_debugName;
}

void WallpaperVideo::setDebugName(const QString &debugName) {
    if (m_debugName == debugName) {
        return;
    }

    m_debugName = debugName;
    qInfo().noquote() << logPrefix() << "debug name assigned";
    emit debugNameChanged();
}

bool WallpaperVideo::isReady() const {
    return m_ready;
}

QString WallpaperVideo::status() const {
    return m_status;
}

QString WallpaperVideo::errorString() const {
    return m_errorString;
}

QString WallpaperVideo::hwdecCurrent() const {
    return m_hwdecCurrent;
}

QSize WallpaperVideo::videoSize() const {
    return m_videoSizeValue;
}

QSize WallpaperVideo::frameSize() const {
    return m_frameSizeValue;
}

WallpaperVideo::FrameSnapshot WallpaperVideo::frameSnapshot() const {
    QMutexLocker locker(&m_frameMutex);
    return FrameSnapshot{
        .textureId = m_textureId,
        .size = m_frameSizeValue,
        .serial = m_frameSerial,
        .hasFrame = m_hasFrame,
    };
}

void WallpaperVideo::ensureInitialized() {
    if (!m_initRetryTimer.isActive()) {
        QMetaObject::invokeMethod(this, &WallpaperVideo::ensureGraphicsReady, Qt::QueuedConnection);
    }
}

void WallpaperVideo::updateRenderTargetHint(QObject *item, const QSize &size) {
    if (item == nullptr) {
        return;
    }

    const quintptr key = reinterpret_cast<quintptr>(item);
    const QSize clamped = size.isValid() ? clampSize(size) : QSize();
    if (clamped.isValid()) {
        if (m_renderTargetHints.value(key) == clamped) {
            return;
        }
        m_renderTargetHints.insert(key, clamped);
    } else if (m_renderTargetHints.remove(key) == 0) {
        return;
    }

    m_forceRender = true;
    ensureInitialized();
    scheduleRender();
}

void WallpaperVideo::removeRenderTargetHint(QObject *item) {
    if (item == nullptr) {
        return;
    }

    const quintptr key = reinterpret_cast<quintptr>(item);
    if (m_renderTargetHints.remove(key) > 0) {
        m_forceRender = true;
        scheduleRender();
    }
}

void WallpaperVideo::updateShareContextHint(QOpenGLContext *context) {
    if (context == nullptr || m_offscreenContext != nullptr || m_shareContextHint == context) {
        return;
    }

    m_shareContextHint = context;
    qInfo().noquote() << logPrefix() << "received scene-graph share context hint" << context;
    ensureInitialized();
}

void WallpaperVideo::ensureGraphicsReady() {
    if (!ensureMpvCore()) {
        return;
    }

    const auto api = QQuickWindow::graphicsApi();
    if (api != QSGRendererInterface::Unknown && api != QSGRendererInterface::OpenGL) {
        setStatus(QStringLiteral("error"));
        setErrorString(QStringLiteral("wallpaper video requires Qt Quick OpenGL backend"));
        return;
    }

    QOpenGLContext *shareContext = m_shareContextHint.data();
    if (shareContext == nullptr) {
        shareContext = QOpenGLContext::globalShareContext();
    }
    if (shareContext == nullptr) {
        if (!m_loggedWaitingForShareContext) {
            qInfo().noquote() << logPrefix() << "waiting for share context";
            m_loggedWaitingForShareContext = true;
        }
        if (!m_initRetryTimer.isActive()) {
            m_initRetryTimer.start();
        }
        return;
    }
    if (m_loggedWaitingForShareContext) {
        qInfo().noquote() << logPrefix() << "share context became available" << shareContext;
        m_loggedWaitingForShareContext = false;
    }

    bool renderContextCreated = false;

    if (m_offscreenSurface == nullptr) {
        auto *surface = new QOffscreenSurface(nullptr, this);
        surface->setFormat(shareContext->format());
        surface->create();
        if (!surface->isValid()) {
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("failed to create wallpaper offscreen surface"));
            surface->deleteLater();
            return;
        }
        m_offscreenSurface = surface;
        qInfo().noquote() << logPrefix() << "offscreen surface created";
    }

    if (m_offscreenContext == nullptr) {
        auto *context = new QOpenGLContext(this);
        context->setFormat(shareContext->format());
        context->setShareContext(shareContext);
        if (!context->create()) {
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("failed to create wallpaper OpenGL context"));
            context->deleteLater();
            return;
        }
        m_offscreenContext = context;
        qInfo().noquote() << logPrefix() << "offscreen OpenGL context created";
    }

    if (m_mpv.renderContext() == nullptr) {
        if (!m_offscreenContext->makeCurrent(m_offscreenSurface)) {
            setStatus(QStringLiteral("error"));
            setErrorString(QStringLiteral("failed to make wallpaper OpenGL context current"));
            return;
        }

        auto *waylandApp = qGuiApp->nativeInterface<QNativeInterface::QWaylandApplication>();
        wl_display *waylandDisplay = waylandApp != nullptr ? waylandApp->display() : nullptr;

        QString error;
        if (!m_mpv.ensureRenderContext(m_offscreenContext, waylandDisplay, &error)) {
            m_offscreenContext->doneCurrent();
            setStatus(QStringLiteral("error"));
            setErrorString(error);
            return;
        }
        m_mpv.setRenderUpdateCallback(&WallpaperVideo::onRenderUpdate, this);
        m_offscreenContext->doneCurrent();
        renderContextCreated = true;
        qInfo().noquote() << logPrefix() << "mpv render context created";
    }

    if (renderContextCreated && !m_source.isEmpty()) {
        loadCurrentSource();
    }
}

void WallpaperVideo::drainEvents() {
    if (!m_mpv.isInitialized()) {
        return;
    }

    for (;;) {
        mpv_event *event = m_mpv.waitEvent(0.0);
        if (event == nullptr || event->event_id == MPV_EVENT_NONE) {
            break;
        }

        switch (event->event_id) {
        case MPV_EVENT_LOG_MESSAGE: {
            const auto *message = static_cast<mpv_event_log_message *>(event->data);
            if (message != nullptr) {
                qInfo().noquote() << logPrefix() << "[mpv]"
                                  << QString::fromUtf8(message->level)
                                  << QString::fromUtf8(message->prefix) + ":"
                                  << QString::fromUtf8(message->text).trimmed();
            }
            break;
        }
        case MPV_EVENT_FILE_LOADED:
            m_forceRender = true;
            if (!m_source.isEmpty()) {
                setStatus(QStringLiteral("loading"));
                setErrorString(QString());
            }
            qInfo().noquote() << logPrefix() << "mpv file loaded";
            applyAudioState();
            scheduleRender();
            break;
        case MPV_EVENT_END_FILE: {
            const auto *endFile = static_cast<mpv_event_end_file *>(event->data);
            if (endFile != nullptr && endFile->reason == MPV_END_FILE_REASON_ERROR) {
                setStatus(QStringLiteral("error"));
                setErrorString(mpvEventErrorString(endFile->error));
                qWarning().noquote() << logPrefix() << "end-file error:"
                                     << mpvEventErrorString(endFile->error);
                if (!m_hasFrame) {
                    setReady(false);
                }
            }
            break;
        }
        case MPV_EVENT_PROPERTY_CHANGE: {
            const auto *property = static_cast<mpv_event_property *>(event->data);
            if (property == nullptr) {
                break;
            }

            const QString name = QString::fromUtf8(property->name);
            if (name == QLatin1String("dwidth")
                && property->format == MPV_FORMAT_INT64 && property->data != nullptr) {
                const auto value = *static_cast<int64_t *>(property->data);
                m_observedDwidth = value;
                updateVideoSize();
            } else if (name == QLatin1String("dheight")
                       && property->format == MPV_FORMAT_INT64 && property->data != nullptr) {
                const auto value = *static_cast<int64_t *>(property->data);
                m_observedDheight = value;
                updateVideoSize();
            } else if (name == QLatin1String("hwdec-current")) {
                QString value;
                if (property->format == MPV_FORMAT_STRING && property->data != nullptr) {
                    const char *raw = *static_cast<char **>(property->data);
                    if (raw != nullptr) {
                        value = QString::fromUtf8(raw);
                    }
                }
                setHwdecCurrent(value);
            }
            break;
        }
        default:
            break;
        }
    }
}

void WallpaperVideo::scheduleRender() {
    if (m_renderScheduled || m_offscreenContext == nullptr || m_mpv.renderContext() == nullptr) {
        return;
    }

    m_renderScheduled = true;
    QMetaObject::invokeMethod(this, &WallpaperVideo::renderFrame, Qt::QueuedConnection);
}

void WallpaperVideo::renderFrame() {
    m_renderScheduled = false;

    if (m_source.isEmpty() || m_offscreenContext == nullptr || m_offscreenSurface == nullptr
        || m_mpv.renderContext() == nullptr) {
        return;
    }

    if (!m_offscreenContext->makeCurrent(m_offscreenSurface)) {
        setStatus(QStringLiteral("error"));
        setErrorString(QStringLiteral("failed to bind wallpaper OpenGL context"));
        return;
    }

    const QSize target = clampSize(targetFrameSize());
    const bool sizeChanged = m_frame.ensureSize(target);
    if (sizeChanged) {
        m_forceRender = true;
    }

    if (!m_frame.isValid()) {
        m_offscreenContext->doneCurrent();
        setStatus(QStringLiteral("error"));
        setErrorString(QStringLiteral("failed to allocate wallpaper framebuffer"));
        return;
    }

    const uint64_t updateFlags = m_mpv.update();
    if ((updateFlags & MPV_RENDER_UPDATE_FRAME) == 0 && !m_forceRender) {
        m_offscreenContext->doneCurrent();
        return;
    }

    m_forceRender = false;

    const mpv_opengl_fbo fbo{
        .fbo = m_frame.handle(),
        .w = target.width(),
        .h = target.height(),
        .internal_format = 0,
    };
    const int flipY = 0;
    const int blockForTargetTime = 0;
    m_mpv.render(fbo, flipY, blockForTargetTime);
    m_offscreenContext->functions()->glFlush();
    m_offscreenContext->doneCurrent();

    const bool frameSizeChangedValue = (m_frameSizeValue != target);
    {
        QMutexLocker locker(&m_frameMutex);
        m_textureId = m_frame.textureId();
        m_frameSizeValue = target;
        m_frameSerial += 1;
        m_hasFrame = true;
    }

    if (!m_ready) {
        setReady(true);
    }
    if (m_status != QLatin1String("ready")) {
        setStatus(QStringLiteral("ready"));
    }
    if (!m_loggedFirstFrame) {
        qInfo().noquote() << logPrefix() << "first frame rendered"
                          << "frameSize=" << target
                          << "videoSize=" << m_videoSizeValue;
        m_loggedFirstFrame = true;
    }

    if (frameSizeChangedValue) {
        emit frameSizeChanged();
    }
    emit frameAvailable();
}

void WallpaperVideo::onWakeup(void *ctx) {
    auto *self = static_cast<WallpaperVideo *>(ctx);
    QMetaObject::invokeMethod(self, &WallpaperVideo::drainEvents, Qt::QueuedConnection);
}

void WallpaperVideo::onRenderUpdate(void *ctx) {
    auto *self = static_cast<WallpaperVideo *>(ctx);
    QMetaObject::invokeMethod(self, &WallpaperVideo::scheduleRender, Qt::QueuedConnection);
}

bool WallpaperVideo::ensureMpvCore() {
    if (m_mpv.isInitialized()) {
        return true;
    }

    QString error;
    if (!m_mpv.initialize(&error)) {
        setStatus(QStringLiteral("error"));
        setErrorString(error);
        return false;
    }

    m_mpv.setWakeupCallback(&WallpaperVideo::onWakeup, this);
    mpv_observe_property(m_mpv.handle(), 1, "dwidth", MPV_FORMAT_INT64);
    mpv_observe_property(m_mpv.handle(), 2, "dheight", MPV_FORMAT_INT64);
    mpv_observe_property(m_mpv.handle(), 3, "hwdec-current", MPV_FORMAT_STRING);
    applyAudioState();
    qInfo().noquote() << logPrefix() << "mpv core initialized";
    return true;
}

void WallpaperVideo::loadCurrentSource() {
    if (!ensureMpvCore() || m_source.isEmpty() || m_mpv.renderContext() == nullptr) {
        return;
    }

    QString localSource = m_source.isLocalFile() ? m_source.toLocalFile() : m_source.toString();
    if (localSource.isEmpty()) {
        setStatus(QStringLiteral("error"));
        setErrorString(QStringLiteral("invalid wallpaper video source"));
        return;
    }
    m_loggedFirstFrame = false;

    QString error;
    if (!m_mpv.command(
            {QStringLiteral("loadfile"), localSource, QStringLiteral("replace")},
            &error
        )) {
        setStatus(QStringLiteral("error"));
        setErrorString(error);
        return;
    }
    qInfo().noquote() << logPrefix() << "loadfile issued:" << localSource;

    applyAudioState();
}

void WallpaperVideo::applyAudioState() {
    if (!m_mpv.isInitialized()) {
        return;
    }

    QString error;
    if (!m_mpv.setPropertyBool("mute", m_muted, &error)) {
        setStatus(QStringLiteral("error"));
        setErrorString(error);
        return;
    }
    if (!m_mpv.setPropertyDouble("volume", static_cast<double>(m_volume), &error)) {
        setStatus(QStringLiteral("error"));
        setErrorString(error);
    }
}

void WallpaperVideo::updateVideoSize() {
    const QSize nextSize = (m_observedDwidth > 0 && m_observedDheight > 0)
        ? QSize(static_cast<int>(m_observedDwidth), static_cast<int>(m_observedDheight))
        : QSize();
    if (nextSize == m_videoSizeValue) {
        return;
    }

    m_videoSizeValue = nextSize;
    m_forceRender = true;
    qInfo().noquote() << logPrefix() << "video size changed to" << m_videoSizeValue;
    emit videoSizeChanged();
    scheduleRender();
}

QSize WallpaperVideo::targetFrameSize() const {
    if (!m_videoSizeValue.isValid()) {
        int width = 1920;
        int height = 1080;
        for (auto it = m_renderTargetHints.cbegin(); it != m_renderTargetHints.cend(); ++it) {
            width = std::max(width, it.value().width());
            height = std::max(height, it.value().height());
        }
        return QSize(width, height);
    }

    const qreal videoAspect = static_cast<qreal>(m_videoSizeValue.width()) / m_videoSizeValue.height();
    int requiredWidth = m_videoSizeValue.width();
    int requiredHeight = m_videoSizeValue.height();

    for (auto it = m_renderTargetHints.cbegin(); it != m_renderTargetHints.cend(); ++it) {
        const QSize hint = it.value();
        if (!hint.isValid()) {
            continue;
        }

        const qreal screenAspect = static_cast<qreal>(hint.width()) / hint.height();
        if (screenAspect > videoAspect) {
            requiredWidth = std::max(requiredWidth, hint.width());
            requiredHeight = std::max(
                requiredHeight,
                static_cast<int>(std::ceil(hint.width() / videoAspect))
            );
        } else {
            requiredHeight = std::max(requiredHeight, hint.height());
            requiredWidth = std::max(
                requiredWidth,
                static_cast<int>(std::ceil(hint.height() * videoAspect))
            );
        }
    }

    return QSize(requiredWidth, requiredHeight);
}

void WallpaperVideo::setReady(bool ready) {
    if (m_ready == ready) {
        return;
    }

    m_ready = ready;
    qInfo().noquote() << logPrefix() << "ready =" << m_ready;
    emit readyChanged();
}

void WallpaperVideo::setStatus(const QString &status) {
    if (m_status == status) {
        return;
    }

    m_status = status;
    qInfo().noquote() << logPrefix() << "status =" << m_status;
    emit statusChanged();
}

void WallpaperVideo::setErrorString(const QString &errorString) {
    if (m_errorString == errorString) {
        return;
    }

    m_errorString = errorString;
    if (!m_errorString.isEmpty()) {
        qWarning().noquote() << logPrefix() << "error =" << m_errorString;
    }
    emit errorStringChanged();
}

void WallpaperVideo::setHwdecCurrent(const QString &hwdecCurrent) {
    if (m_hwdecCurrent == hwdecCurrent) {
        return;
    }

    m_hwdecCurrent = hwdecCurrent;
    qInfo().noquote() << logPrefix() << "hwdec-current ="
                      << (m_hwdecCurrent.isEmpty() ? QStringLiteral("<none>") : m_hwdecCurrent);
    emit hwdecCurrentChanged();
}

void WallpaperVideo::clearVideoSize() {
    m_observedDwidth = 0;
    m_observedDheight = 0;
    if (m_videoSizeValue.isValid()) {
        m_videoSizeValue = QSize();
        emit videoSizeChanged();
    }
}

QString WallpaperVideo::logPrefix() const {
    if (m_debugName.isEmpty()) {
        return QStringLiteral("[wallpaper-video]");
    }
    return QStringLiteral("[wallpaper-video %1]").arg(m_debugName);
}
