// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QPointer>
#include <QQuickItem>
#include <QRectF>
#include <QtQmlIntegration/qqmlintegration.h>

#include "WallpaperVideo.hpp"

class QSGTexture;

class WallpaperVideoItem : public QQuickItem {
    Q_OBJECT
    QML_ELEMENT

    Q_PROPERTY(WallpaperVideo *controller READ controller WRITE setController NOTIFY controllerChanged FINAL)
    Q_PROPERTY(QRectF cropRect READ cropRect WRITE setCropRect NOTIFY cropRectChanged FINAL)
    Q_PROPERTY(bool ready READ isReady NOTIFY readyChanged FINAL)

public:
    explicit WallpaperVideoItem(QQuickItem *parent = nullptr);
    ~WallpaperVideoItem() override;

    [[nodiscard]] WallpaperVideo *controller() const;
    void setController(WallpaperVideo *controller);

    [[nodiscard]] QRectF cropRect() const;
    void setCropRect(const QRectF &cropRect);

    [[nodiscard]] bool isReady() const;

signals:
    void controllerChanged();
    void cropRectChanged();
    void readyChanged();

protected:
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *data) override;
    void releaseResources() override;

private:
    void reconnectController();
    void syncHint();
    QSize pixelSizeHint() const;
    QRectF sourceRectFor(const QSize &frameSize) const;
    void clearTexture();

    WallpaperVideo *m_controller = nullptr;
    QRectF m_cropRect;
    QMetaObject::Connection m_frameConnection;
    QMetaObject::Connection m_readyConnection;
    QMetaObject::Connection m_statusConnection;
    QSGTexture *m_texture = nullptr;
    quint64 m_textureSerial = 0;
    QPointer<QQuickWindow> m_window;
};
