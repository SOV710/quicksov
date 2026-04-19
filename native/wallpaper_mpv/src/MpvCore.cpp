// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "MpvCore.hpp"

#include <clocale>
#include <vector>

#include <QByteArray>
#include <QDebug>
#include <QOpenGLContext>

namespace {

QString mpvErrorString(int code) {
    const char *message = mpv_error_string(code);
    if (message == nullptr || *message == '\0') {
        return QStringLiteral("unknown mpv error");
    }
    return QString::fromUtf8(message);
}

bool setOptionString(mpv_handle *handle, const char *name, const char *value, QString *error) {
    const int rc = mpv_set_option_string(handle, name, value);
    if (rc >= 0) {
        return true;
    }

    if (error != nullptr) {
        *error = QStringLiteral("mpv option %1=%2 failed: %3")
                     .arg(QString::fromUtf8(name), QString::fromUtf8(value), mpvErrorString(rc));
    }
    return false;
}

} // namespace

MpvCore::~MpvCore() {
    destroyRenderContext();
    if (m_handle != nullptr) {
        mpv_terminate_destroy(m_handle);
        m_handle = nullptr;
    }
}

bool MpvCore::initialize(QString *error) {
    if (m_handle != nullptr) {
        return true;
    }

    std::setlocale(LC_NUMERIC, "C");
    qInfo().noquote() << "[wallpaper-video] forced LC_NUMERIC=C before mpv_create";

    m_handle = mpv_create();
    if (m_handle == nullptr) {
        if (error != nullptr) {
            *error = QStringLiteral("mpv_create() failed");
        }
        return false;
    }

    const char *const options[][2] = {
        {"terminal", "no"},
        {"config", "no"},
        {"vo", "libmpv"},
        {"hwdec", "auto-safe"},
        {"loop-file", "inf"},
        {"keep-open", "yes"},
        {"idle", "yes"},
        {"osc", "no"},
        {"audio-display", "no"},
        {"input-default-bindings", "no"},
        {"input-vo-keyboard", "no"},
        {"force-window", "no"},
    };

    for (const auto &entry : options) {
        if (!setOptionString(m_handle, entry[0], entry[1], error)) {
            return false;
        }
    }

    const int initRc = mpv_initialize(m_handle);
    if (initRc < 0) {
        if (error != nullptr) {
            *error = QStringLiteral("mpv_initialize() failed: %1").arg(mpvErrorString(initRc));
        }
        return false;
    }

    mpv_request_log_messages(m_handle, "info");
    return true;
}

bool MpvCore::isInitialized() const {
    return m_handle != nullptr;
}

mpv_handle *MpvCore::handle() const {
    return m_handle;
}

mpv_render_context *MpvCore::renderContext() const {
    return m_renderContext;
}

void MpvCore::setWakeupCallback(void (*callback)(void *), void *ctx) {
    if (m_handle != nullptr) {
        mpv_set_wakeup_callback(m_handle, callback, ctx);
    }
}

void MpvCore::setRenderUpdateCallback(void (*callback)(void *), void *ctx) {
    if (m_renderContext != nullptr) {
        mpv_render_context_set_update_callback(m_renderContext, callback, ctx);
    }
}

bool MpvCore::ensureRenderContext(
    QOpenGLContext *context,
    wl_display *waylandDisplay,
    QString *error
) {
    if (m_renderContext != nullptr) {
        return true;
    }

    mpv_opengl_init_params glInit{
        .get_proc_address = &MpvCore::getProcAddress,
        .get_proc_address_ctx = context,
    };
    const char *apiType = MPV_RENDER_API_TYPE_OPENGL;

    std::vector<mpv_render_param> params;
    params.push_back({MPV_RENDER_PARAM_API_TYPE, const_cast<char *>(apiType)});
    params.push_back({MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &glInit});
    if (waylandDisplay != nullptr) {
        params.push_back({MPV_RENDER_PARAM_WL_DISPLAY, waylandDisplay});
    }
    params.push_back({MPV_RENDER_PARAM_INVALID, nullptr});

    const int rc = mpv_render_context_create(&m_renderContext, m_handle, params.data());
    if (rc >= 0) {
        return true;
    }

    if (error != nullptr) {
        *error = QStringLiteral("mpv_render_context_create() failed: %1").arg(mpvErrorString(rc));
    }
    return false;
}

void MpvCore::destroyRenderContext() {
    if (m_renderContext != nullptr) {
        mpv_render_context_free(m_renderContext);
        m_renderContext = nullptr;
    }
}

bool MpvCore::setPropertyBool(const char *name, bool value, QString *error) {
    int flag = value ? 1 : 0;
    const int rc = mpv_set_property(m_handle, name, MPV_FORMAT_FLAG, &flag);
    if (rc >= 0) {
        return true;
    }

    if (error != nullptr) {
        *error = QStringLiteral("mpv property %1 failed: %2")
                     .arg(QString::fromUtf8(name), mpvErrorString(rc));
    }
    return false;
}

bool MpvCore::setPropertyDouble(const char *name, double value, QString *error) {
    const int rc = mpv_set_property(m_handle, name, MPV_FORMAT_DOUBLE, &value);
    if (rc >= 0) {
        return true;
    }

    if (error != nullptr) {
        *error = QStringLiteral("mpv property %1 failed: %2")
                     .arg(QString::fromUtf8(name), mpvErrorString(rc));
    }
    return false;
}

bool MpvCore::command(const QStringList &args, QString *error) {
    std::vector<QByteArray> storage;
    storage.reserve(static_cast<size_t>(args.size()));
    std::vector<const char *> argv;
    argv.reserve(static_cast<size_t>(args.size()) + 1U);

    for (const QString &arg : args) {
        storage.push_back(arg.toUtf8());
        argv.push_back(storage.back().constData());
    }
    argv.push_back(nullptr);

    const int rc = mpv_command(m_handle, argv.data());
    if (rc >= 0) {
        return true;
    }

    if (error != nullptr) {
        *error = QStringLiteral("mpv command failed: %1").arg(mpvErrorString(rc));
    }
    return false;
}

mpv_event *MpvCore::waitEvent(double timeout) const {
    return mpv_wait_event(m_handle, timeout);
}

uint64_t MpvCore::update() const {
    if (m_renderContext == nullptr) {
        return 0;
    }
    return mpv_render_context_update(m_renderContext);
}

void MpvCore::render(const mpv_opengl_fbo &fbo, int flipY, int blockForTargetTime) {
    mpv_render_param params[] = {
        {MPV_RENDER_PARAM_OPENGL_FBO, const_cast<mpv_opengl_fbo *>(&fbo)},
        {MPV_RENDER_PARAM_FLIP_Y, &flipY},
        {MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME, &blockForTargetTime},
        {MPV_RENDER_PARAM_INVALID, nullptr},
    };
    mpv_render_context_render(m_renderContext, params);
}

void *MpvCore::getProcAddress(void *ctx, const char *name) {
    auto *context = static_cast<QOpenGLContext *>(ctx);
    return reinterpret_cast<void *>(context->getProcAddress(name));
}
