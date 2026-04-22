# SPDX-FileCopyrightText: 2026 SOV710
#
# SPDX-License-Identifier: GPL-3.0-or-later

cmake_minimum_required(VERSION 3.24)

project(qsov_wallpaper_renderer LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 REQUIRED COMPONENTS Core Gui Network Svg Qml)
find_package(PkgConfig REQUIRED)
pkg_check_modules(WAYLAND_CLIENT IMPORTED_TARGET wayland-client REQUIRED)
pkg_check_modules(AVFORMAT IMPORTED_TARGET libavformat REQUIRED)
pkg_check_modules(AVCODEC IMPORTED_TARGET libavcodec REQUIRED)
pkg_check_modules(AVUTIL IMPORTED_TARGET libavutil REQUIRED)
pkg_check_modules(SWSCALE IMPORTED_TARGET libswscale REQUIRED)
pkg_check_modules(GBM IMPORTED_TARGET gbm REQUIRED)
pkg_check_modules(LIBDRM IMPORTED_TARGET libdrm REQUIRED)
pkg_check_modules(EGL IMPORTED_TARGET egl REQUIRED)
pkg_check_modules(GLESV2 IMPORTED_TARGET glesv2 REQUIRED)
pkg_check_modules(LIBPLACEBO IMPORTED_TARGET libplacebo REQUIRED)

qt_standard_project_setup(REQUIRES 6.8)

set(LAYER_SHELL_XML "${CMAKE_CURRENT_SOURCE_DIR}/proto/wlr-layer-shell-unstable-v1.xml")
set(GENERATED_DIR "${CMAKE_CURRENT_BINARY_DIR}/generated")
set(WALLPAPER_CONTRACT_JSON "${CMAKE_CURRENT_SOURCE_DIR}/../../../protocol/wallpaper-contract.json")
set(LAYER_SHELL_HEADER "${GENERATED_DIR}/wlr-layer-shell-unstable-v1-client-protocol.h")
set(LAYER_SHELL_SOURCE "${GENERATED_DIR}/wlr-layer-shell-unstable-v1-client-protocol.c")
set(WALLPAPER_CONTRACT_HEADER "${GENERATED_DIR}/WallpaperContractConfig.hpp")
find_file(
    LINUX_DMABUF_XML
    NAMES linux-dmabuf-v1.xml
    PATHS
        /usr/share/wayland-protocols/stable/linux-dmabuf
    REQUIRED
)
set(LINUX_DMABUF_HEADER "${GENERATED_DIR}/linux-dmabuf-v1-client-protocol.h")
set(LINUX_DMABUF_SOURCE "${GENERATED_DIR}/linux-dmabuf-v1-client-protocol.c")
find_file(
    XDG_SHELL_XML
    NAMES xdg-shell.xml
    PATHS
        /usr/share/wayland-protocols/stable/xdg-shell
        /usr/share/wayland-protocols/unstable/xdg-shell
    REQUIRED
)
set(XDG_SHELL_HEADER "${GENERATED_DIR}/xdg-shell-client-protocol.h")
set(XDG_SHELL_SOURCE "${GENERATED_DIR}/xdg-shell-client-protocol.c")

file(MAKE_DIRECTORY "${GENERATED_DIR}")

file(READ "${WALLPAPER_CONTRACT_JSON}" WALLPAPER_CONTRACT)
string(JSON WALLPAPER_PROTO_VERSION GET "${WALLPAPER_CONTRACT}" protocol proto_version)
string(JSON WALLPAPER_TOPIC GET "${WALLPAPER_CONTRACT}" protocol topic)
string(JSON WALLPAPER_RENDERER_BINARY_NAME GET "${WALLPAPER_CONTRACT}" renderer binary_name)
string(JSON WALLPAPER_RENDERER_CLIENT_NAME GET "${WALLPAPER_CONTRACT}" renderer client_name)
string(JSON WALLPAPER_RENDERER_CLIENT_VERSION GET "${WALLPAPER_CONTRACT}" renderer client_version)
string(JSON WALLPAPER_LAYER_NAMESPACE GET "${WALLPAPER_CONTRACT}" renderer layer_namespace)
string(JSON WALLPAPER_SOFTWARE_DECODE_BACKEND GET "${WALLPAPER_CONTRACT}" decoder software_backend)
string(JSON WALLPAPER_DECODE_CATALOG_LENGTH LENGTH "${WALLPAPER_CONTRACT}" decoder catalog)
string(JSON WALLPAPER_DECODE_DEFAULT_ORDER_LENGTH LENGTH "${WALLPAPER_CONTRACT}" decoder default_order)

math(EXPR WALLPAPER_DECODE_CATALOG_LAST "${WALLPAPER_DECODE_CATALOG_LENGTH} - 1")
set(WALLPAPER_DECODE_CATALOG_INIT "")
foreach(index RANGE 0 ${WALLPAPER_DECODE_CATALOG_LAST})
    string(JSON decode_backend GET "${WALLPAPER_CONTRACT}" decoder catalog ${index})
    string(APPEND WALLPAPER_DECODE_CATALOG_INIT "    \"${decode_backend}\",\n")
endforeach()

math(EXPR WALLPAPER_DECODE_DEFAULT_ORDER_LAST "${WALLPAPER_DECODE_DEFAULT_ORDER_LENGTH} - 1")
set(WALLPAPER_DECODE_DEFAULT_ORDER_INIT "")
foreach(index RANGE 0 ${WALLPAPER_DECODE_DEFAULT_ORDER_LAST})
    string(JSON decode_backend GET "${WALLPAPER_CONTRACT}" decoder default_order ${index})
    string(APPEND WALLPAPER_DECODE_DEFAULT_ORDER_INIT "    \"${decode_backend}\",\n")
endforeach()

configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/src/WallpaperContractConfig.hpp.in"
    "${WALLPAPER_CONTRACT_HEADER}"
    @ONLY
)

add_custom_command(
    OUTPUT
        "${LAYER_SHELL_HEADER}"
        "${LAYER_SHELL_SOURCE}"
        "${LINUX_DMABUF_HEADER}"
        "${LINUX_DMABUF_SOURCE}"
        "${XDG_SHELL_HEADER}"
        "${XDG_SHELL_SOURCE}"
    COMMAND wayland-scanner client-header "${LAYER_SHELL_XML}" "${LAYER_SHELL_HEADER}"
    COMMAND wayland-scanner private-code "${LAYER_SHELL_XML}" "${LAYER_SHELL_SOURCE}"
    COMMAND wayland-scanner client-header "${LINUX_DMABUF_XML}" "${LINUX_DMABUF_HEADER}"
    COMMAND wayland-scanner private-code "${LINUX_DMABUF_XML}" "${LINUX_DMABUF_SOURCE}"
    COMMAND wayland-scanner client-header "${XDG_SHELL_XML}" "${XDG_SHELL_HEADER}"
    COMMAND wayland-scanner private-code "${XDG_SHELL_XML}" "${XDG_SHELL_SOURCE}"
    DEPENDS
        "${LAYER_SHELL_XML}"
        "${LINUX_DMABUF_XML}"
        "${XDG_SHELL_XML}"
    VERBATIM
)

add_executable(qsov-wallpaper-renderer
    src/main.cpp
    src/WallpaperContract.hpp
    src/Runtime.hpp
    src/ProtocolClient.hpp
    src/ProtocolClient.cpp
    src/SnapshotModel.hpp
    src/SnapshotModel.cpp
    src/GpuCompositor.hpp
    src/GpuCompositor.cpp
    src/SourceSession.hpp
    src/SourceSession.cpp
    src/OutputSurface.hpp
    src/OutputSurfaceCore.cpp
    src/OutputSurfaceBuffers.cpp
    src/OutputSurfaceRender.cpp
    src/WaylandRenderer.hpp
    src/WaylandRenderer.cpp
    src/WaylandRendererEvents.cpp
    src/libplacebo_libav_impl.c
    "${CMAKE_CURRENT_SOURCE_DIR}/../decoder/ffmpeg/VideoDecoder.hpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/../decoder/ffmpeg/VideoDecoderCore.cpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/../decoder/ffmpeg/VideoDecoderDecode.cpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/../decoder/ffmpeg/VideoDecoderUtils.cpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/../decoder/ffmpeg/VideoDecoderInternal.hpp"
    "${LAYER_SHELL_SOURCE}"
    "${LINUX_DMABUF_SOURCE}"
    "${XDG_SHELL_SOURCE}"
)

target_include_directories(qsov-wallpaper-renderer
    PRIVATE
        "${GENERATED_DIR}"
        "${CMAKE_CURRENT_SOURCE_DIR}/src"
        "${CMAKE_CURRENT_SOURCE_DIR}/../decoder/ffmpeg"
)

target_link_libraries(qsov-wallpaper-renderer
    PRIVATE
        Qt::Core
        Qt::Gui
        Qt::Network
        Qt::Svg
        Qt::Qml
        PkgConfig::WAYLAND_CLIENT
        PkgConfig::AVFORMAT
        PkgConfig::AVCODEC
        PkgConfig::AVUTIL
        PkgConfig::SWSCALE
        PkgConfig::GBM
        PkgConfig::LIBDRM
        PkgConfig::EGL
        PkgConfig::GLESV2
        PkgConfig::LIBPLACEBO
        ${CMAKE_DL_LIBS}
)

set_target_properties(qsov-wallpaper-renderer PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)
