// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QString>
#include <QStringList>

#include <mpv/client.h>
#include <mpv/render.h>
#include <mpv/render_gl.h>

struct wl_display;
class QOpenGLContext;

class MpvCore final {
public:
    MpvCore() = default;
    ~MpvCore();

    [[nodiscard]] bool initialize(QString *error);
    [[nodiscard]] bool isInitialized() const;
    [[nodiscard]] mpv_handle *handle() const;
    [[nodiscard]] mpv_render_context *renderContext() const;

    void setWakeupCallback(void (*callback)(void *), void *ctx);
    void setRenderUpdateCallback(void (*callback)(void *), void *ctx);

    [[nodiscard]] bool ensureRenderContext(
        QOpenGLContext *context,
        wl_display *waylandDisplay,
        QString *error
    );
    void destroyRenderContext();

    [[nodiscard]] bool setPropertyBool(const char *name, bool value, QString *error);
    [[nodiscard]] bool setPropertyDouble(const char *name, double value, QString *error);
    [[nodiscard]] bool command(const QStringList &args, QString *error);

    [[nodiscard]] mpv_event *waitEvent(double timeout) const;
    [[nodiscard]] uint64_t update() const;
    void render(const mpv_opengl_fbo &fbo, int flipY, int blockForTargetTime);

private:
    static void *getProcAddress(void *ctx, const char *name);

    mpv_handle *m_handle = nullptr;
    mpv_render_context *m_renderContext = nullptr;
};
