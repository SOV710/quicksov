// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "WallpaperVideoItem.hpp"

#include <algorithm>
#include <cmath>

#include <QQuickWindow>
#include <QSGRendererInterface>
#include <QSGNode>
#include <QSGSimpleTextureNode>
#include <QSGTexture>
#include <QtQuick/qsgtexture_platform.h>

WallpaperVideoItem::WallpaperVideoItem(QQuickItem *parent)
    : QQuickItem(parent) {
    setFlag(ItemHasContents, true);
    connect(this, &QQuickItem::windowChanged, this, &WallpaperVideoItem::handleWindowChanged);
}

WallpaperVideoItem::~WallpaperVideoItem() {
    if (m_controller != nullptr) {
        m_controller->removeRenderTargetHint(this);
    }
    clearTexture();
}

WallpaperVideo *WallpaperVideoItem::controller() const {
    return m_controller;
}

void WallpaperVideoItem::setController(WallpaperVideo *controller) {
    if (m_controller == controller) {
        return;
    }

    if (m_controller != nullptr) {
        m_controller->removeRenderTargetHint(this);
    }

    m_controller = controller;
    reconnectController();
    syncHint();
    emit controllerChanged();
    emit readyChanged();
    update();
}

QRectF WallpaperVideoItem::cropRect() const {
    return m_cropRect;
}

void WallpaperVideoItem::setCropRect(const QRectF &cropRect) {
    if (m_cropRect == cropRect) {
        return;
    }

    m_cropRect = cropRect;
    emit cropRectChanged();
    update();
}

bool WallpaperVideoItem::isReady() const {
    return m_controller != nullptr && m_controller->isReady();
}

void WallpaperVideoItem::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    syncHint();
    update();
}

QSGNode *WallpaperVideoItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *data) {
    Q_UNUSED(data)

    if (m_controller == nullptr || window() == nullptr) {
        clearTexture();
        delete oldNode;
        return nullptr;
    }

    auto *context = static_cast<QOpenGLContext *>(
        window()->rendererInterface()->getResource(window(), QSGRendererInterface::OpenGLContextResource)
    );
    if (context != nullptr) {
        QPointer<WallpaperVideo> controller = m_controller;
        QMetaObject::invokeMethod(
            m_controller,
            [controller, context]() {
                if (controller != nullptr) {
                    controller->updateShareContextHint(context);
                }
            },
            Qt::QueuedConnection
        );
    }

    const WallpaperVideo::FrameSnapshot snapshot = m_controller->frameSnapshot();
    if (!snapshot.hasFrame || snapshot.textureId == 0 || !snapshot.size.isValid()) {
        clearTexture();
        delete oldNode;
        return nullptr;
    }

    auto *node = static_cast<QSGSimpleTextureNode *>(oldNode);
    if (node == nullptr) {
        node = new QSGSimpleTextureNode();
        node->setOwnsTexture(false);
    }

    if (m_texture == nullptr || m_textureId != snapshot.textureId || m_textureSize != snapshot.size
        || m_textureSerial != snapshot.serial) {
        clearTexture();
        m_texture = QNativeInterface::QSGOpenGLTexture::fromNative(
            snapshot.textureId,
            window(),
            snapshot.size
        );
        m_textureId = snapshot.textureId;
        m_textureSize = snapshot.size;
        m_textureSerial = snapshot.serial;
    }

    if (m_texture == nullptr) {
        delete node;
        return nullptr;
    }

    node->setTexture(m_texture);
    node->setFiltering(QSGTexture::Linear);
    node->setRect(boundingRect());
    node->setSourceRect(sourceRectFor(snapshot.size));
    return node;
}

void WallpaperVideoItem::releaseResources() {
    clearTexture();
}

void WallpaperVideoItem::handleWindowChanged(QQuickWindow *window) {
    m_window = window;
    syncHint();
    if (m_controller != nullptr) {
        m_controller->ensureInitialized();
    }
    update();
}

void WallpaperVideoItem::reconnectController() {
    if (m_frameConnection) {
        disconnect(m_frameConnection);
    }
    if (m_readyConnection) {
        disconnect(m_readyConnection);
    }
    if (m_statusConnection) {
        disconnect(m_statusConnection);
    }

    if (m_controller == nullptr) {
        return;
    }

    m_frameConnection = connect(m_controller, &WallpaperVideo::frameAvailable, this, [this]() {
        update();
    });
    m_readyConnection = connect(m_controller, &WallpaperVideo::readyChanged, this, [this]() {
        emit readyChanged();
        update();
    });
    m_statusConnection = connect(m_controller, &WallpaperVideo::statusChanged, this, [this]() {
        update();
    });
}

void WallpaperVideoItem::syncHint() {
    if (m_controller == nullptr) {
        return;
    }

    m_controller->updateRenderTargetHint(this, pixelSizeHint());
    m_controller->ensureInitialized();
}

QSize WallpaperVideoItem::pixelSizeHint() const {
    QQuickWindow *itemWindow = window();
    if (itemWindow == nullptr) {
        return QSize();
    }
    const qreal dpr = itemWindow != nullptr ? itemWindow->effectiveDevicePixelRatio() : 1.0;
    return QSize(
        std::max(1, static_cast<int>(std::ceil(width() * dpr))),
        std::max(1, static_cast<int>(std::ceil(height() * dpr)))
    );
}

QRectF WallpaperVideoItem::sourceRectFor(const QSize &frameSize) const {
    if (!frameSize.isValid()) {
        return QRectF();
    }

    const QRectF fullRect(0.0, 0.0, frameSize.width(), frameSize.height());
    if (m_cropRect.width() > 0.0 && m_cropRect.height() > 0.0) {
        QRectF rect(
            m_cropRect.x() * frameSize.width(),
            m_cropRect.y() * frameSize.height(),
            m_cropRect.width() * frameSize.width(),
            m_cropRect.height() * frameSize.height()
        );
        rect = rect.intersected(fullRect);
        if (rect.width() > 0.0 && rect.height() > 0.0) {
            return rect;
        }
        return fullRect;
    }

    if (width() <= 0.0 || height() <= 0.0) {
        return fullRect;
    }

    const qreal frameAspect = static_cast<qreal>(frameSize.width()) / frameSize.height();
    const qreal itemAspect = width() / height();

    if (itemAspect > frameAspect) {
        const qreal visibleHeight = frameSize.width() / itemAspect;
        const qreal top = (frameSize.height() - visibleHeight) * 0.5;
        return QRectF(0.0, top, frameSize.width(), visibleHeight);
    }

    const qreal visibleWidth = frameSize.height() * itemAspect;
    const qreal left = (frameSize.width() - visibleWidth) * 0.5;
    return QRectF(left, 0.0, visibleWidth, frameSize.height());
}

void WallpaperVideoItem::clearTexture() {
    delete m_texture;
    m_texture = nullptr;
    m_textureId = 0;
    m_textureSize = QSize();
    m_textureSerial = 0;
}
