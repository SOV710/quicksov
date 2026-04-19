# SPDX-FileCopyrightText: 2026 SOV710
#
# SPDX-License-Identifier: GPL-3.0-or-later

cmake_minimum_required(VERSION 3.24)

project(quicksov_wallpaper_ffmpeg LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_package(Qt6 REQUIRED COMPONENTS Core Gui Qml Quick)
find_package(PkgConfig REQUIRED)
pkg_check_modules(AVFORMAT IMPORTED_TARGET libavformat REQUIRED)
pkg_check_modules(AVCODEC IMPORTED_TARGET libavcodec REQUIRED)
pkg_check_modules(AVUTIL IMPORTED_TARGET libavutil REQUIRED)
pkg_check_modules(SWSCALE IMPORTED_TARGET libswscale REQUIRED)

if(NOT QT_QML_OUTPUT_DIRECTORY)
    set(QT_QML_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/qml")
endif()

qt_standard_project_setup(REQUIRES 6.8)

qt_add_qml_module(quicksov_wallpaper_ffmpeg
    URI Quicksov.WallpaperFfmpeg
    VERSION 1.0
    SOURCES
        src/WallpaperVideo.cpp
        src/WallpaperVideoItem.cpp
)

set(QUICKSOV_WALLPAPER_MODULE_DIR "${QT_QML_OUTPUT_DIRECTORY}/Quicksov/WallpaperFfmpeg")

set_target_properties(quicksov_wallpaper_ffmpeg PROPERTIES
    LIBRARY_OUTPUT_DIRECTORY "${QUICKSOV_WALLPAPER_MODULE_DIR}"
    RUNTIME_OUTPUT_DIRECTORY "${QUICKSOV_WALLPAPER_MODULE_DIR}"
    BUILD_RPATH "$ORIGIN"
    INSTALL_RPATH "$ORIGIN"
)
set_target_properties(quicksov_wallpaper_ffmpegplugin PROPERTIES
    LIBRARY_OUTPUT_DIRECTORY "${QUICKSOV_WALLPAPER_MODULE_DIR}"
    RUNTIME_OUTPUT_DIRECTORY "${QUICKSOV_WALLPAPER_MODULE_DIR}"
    BUILD_RPATH "$ORIGIN"
    INSTALL_RPATH "$ORIGIN"
)

target_include_directories(quicksov_wallpaper_ffmpeg PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/src")

target_link_libraries(quicksov_wallpaper_ffmpeg
    PRIVATE
        Qt::Core
        Qt::Gui
        Qt::Qml
        Qt::Quick
        PkgConfig::AVFORMAT
        PkgConfig::AVCODEC
        PkgConfig::AVUTIL
        PkgConfig::SWSCALE
        ${CMAKE_DL_LIBS}
)
