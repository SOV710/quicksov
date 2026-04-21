// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "Runtime.hpp"

#include <QCoreApplication>
#include <QJsonDocument>

namespace quicksov::wallpaper::renderer {

WallpaperProtocolClient::WallpaperProtocolClient(QString socketPath, QObject *parent)
    : QObject(parent)
    , m_socketPath(std::move(socketPath)) {
    connect(&m_socket, &QLocalSocket::connected, this, &WallpaperProtocolClient::onConnected);
    connect(&m_socket, &QLocalSocket::readyRead, this, &WallpaperProtocolClient::onReadyRead);
    connect(&m_socket, &QLocalSocket::disconnected, this, [this]() {
        emit fatalError(QStringLiteral("daemon socket disconnected"));
    });
    connect(
        &m_socket,
        &QLocalSocket::errorOccurred,
        this,
        [this](QLocalSocket::LocalSocketError) {
            emit fatalError(m_socket.errorString());
        }
    );
}

void WallpaperProtocolClient::start() {
    qInfo().noquote() << kLogPrefix << "connecting to daemon socket" << m_socketPath;
    m_socket.connectToServer(m_socketPath);
}

void WallpaperProtocolClient::sendJson(const QJsonObject &object) {
    const QByteArray encoded = QJsonDocument(object).toJson(QJsonDocument::Compact) + '\n';
    m_socket.write(encoded);
    m_socket.flush();
}

void WallpaperProtocolClient::onConnected() {
    sendJson(QJsonObject{
        {QStringLiteral("proto_version"), QStringLiteral("qsov/1")},
        {QStringLiteral("client_name"), QStringLiteral("qsov-wallpaper-renderer")},
        {QStringLiteral("client_version"), QStringLiteral("0.1")},
    });
}

void WallpaperProtocolClient::onReadyRead() {
    m_buffer += m_socket.readAll();

    qsizetype newline = 0;
    while ((newline = m_buffer.indexOf('\n')) >= 0) {
        const QByteArray line = m_buffer.left(newline).trimmed();
        m_buffer.remove(0, newline + 1);
        if (line.isEmpty()) {
            continue;
        }

        const QJsonDocument doc = QJsonDocument::fromJson(line);
        if (!doc.isObject()) {
            emit fatalError(QStringLiteral("received malformed daemon JSON"));
            return;
        }

        const QJsonObject object = doc.object();
        if (object.value(QStringLiteral("_type")).toString() == QStringLiteral("HelloAck")) {
            sendJson(QJsonObject{
                {QStringLiteral("id"), 0},
                {QStringLiteral("kind"), 5},
                {QStringLiteral("topic"), QStringLiteral("wallpaper")},
                {QStringLiteral("action"), QStringLiteral("")},
                {QStringLiteral("payload"), QJsonValue::Null},
            });
            continue;
        }

        const int kind = object.value(QStringLiteral("kind")).toInt(-1);
        const QString topic = object.value(QStringLiteral("topic")).toString();
        if (kind == 3 && topic == QStringLiteral("wallpaper")) {
            emit snapshotReceived(object.value(QStringLiteral("payload")).toObject());
        } else if (kind == 2) {
            emit fatalError(QStringLiteral("daemon returned ERR for wallpaper subscription"));
        }
    }
}

WallpaperRuntime::WallpaperRuntime(QObject *parent)
    : QObject(parent)
    , m_protocol(defaultSocketPath(), this) {
    connect(&m_protocol, &WallpaperProtocolClient::snapshotReceived, this, [this](const QJsonObject &payload) {
        m_renderer.applySnapshot(parseSnapshot(payload));
    });
    connect(&m_protocol, &WallpaperProtocolClient::fatalError, this, &WallpaperRuntime::fail);
    connect(&m_renderer, &WaylandRenderer::fatalError, this, &WallpaperRuntime::fail);
}

int WallpaperRuntime::start() {
    QString error;
    if (!m_renderer.initialize(&error)) {
        fail(error);
        return 1;
    }

    m_protocol.start();
    return 0;
}

void WallpaperRuntime::fail(const QString &message) {
    qCritical().noquote() << kLogPrefix << message;
    QCoreApplication::exit(1);
}

} // namespace quicksov::wallpaper::renderer
