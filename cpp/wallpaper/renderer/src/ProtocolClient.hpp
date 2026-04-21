// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QByteArray>
#include <QJsonObject>
#include <QLocalSocket>
#include <QObject>

namespace quicksov::wallpaper::renderer {

class WallpaperProtocolClient final : public QObject {
    Q_OBJECT

public:
    explicit WallpaperProtocolClient(QString socketPath, QObject *parent = nullptr);

    void start();

signals:
    void snapshotReceived(const QJsonObject &payload);
    void fatalError(const QString &message);

private:
    void sendJson(const QJsonObject &object);
    void onConnected();
    void onReadyRead();

    QString m_socketPath;
    QLocalSocket m_socket;
    QByteArray m_buffer;
};

} // namespace quicksov::wallpaper::renderer
