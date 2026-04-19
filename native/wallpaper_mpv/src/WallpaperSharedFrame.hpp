// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <memory>

#include <QOpenGLFramebufferObject>
#include <QSize>

class WallpaperSharedFrame final {
public:
    [[nodiscard]] bool ensureSize(const QSize &size) {
        if (m_fbo && m_fbo->size() == size) {
            return false;
        }

        m_fbo.reset();

        QOpenGLFramebufferObjectFormat format;
        format.setAttachment(QOpenGLFramebufferObject::NoAttachment);
        format.setTextureTarget(GL_TEXTURE_2D);
        m_fbo = std::make_unique<QOpenGLFramebufferObject>(size, format);
        return true;
    }

    void reset() {
        m_fbo.reset();
    }

    [[nodiscard]] bool isValid() const {
        return m_fbo && m_fbo->isValid();
    }

    [[nodiscard]] QSize size() const {
        return m_fbo ? m_fbo->size() : QSize();
    }

    [[nodiscard]] GLuint textureId() const {
        return m_fbo ? m_fbo->texture() : 0U;
    }

    [[nodiscard]] int handle() const {
        return m_fbo ? m_fbo->handle() : 0;
    }

private:
    std::unique_ptr<QOpenGLFramebufferObject> m_fbo;
};
