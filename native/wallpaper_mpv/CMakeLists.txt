cmake_minimum_required(VERSION 3.24)

project(quicksov_wallpaper_mpv LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_package(Qt6 REQUIRED COMPONENTS Core Gui Qml Quick OpenGL)
find_package(PkgConfig REQUIRED)
pkg_check_modules(MPV IMPORTED_TARGET mpv REQUIRED)

if(NOT QT_QML_OUTPUT_DIRECTORY)
    set(QT_QML_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/qml")
endif()

qt_standard_project_setup(REQUIRES 6.8)

qt_add_qml_module(quicksov_wallpaper_mpv
    URI Quicksov.WallpaperMpv
    VERSION 1.0
    SOURCES
        src/MpvCore.cpp
        src/WallpaperVideo.cpp
        src/WallpaperVideoItem.cpp
)

set(QUICKSOV_WALLPAPER_MODULE_DIR "${QT_QML_OUTPUT_DIRECTORY}/Quicksov/WallpaperMpv")

set_target_properties(quicksov_wallpaper_mpv PROPERTIES
    LIBRARY_OUTPUT_DIRECTORY "${QUICKSOV_WALLPAPER_MODULE_DIR}"
    RUNTIME_OUTPUT_DIRECTORY "${QUICKSOV_WALLPAPER_MODULE_DIR}"
    BUILD_RPATH "$ORIGIN"
    INSTALL_RPATH "$ORIGIN"
)
set_target_properties(quicksov_wallpaper_mpvplugin PROPERTIES
    LIBRARY_OUTPUT_DIRECTORY "${QUICKSOV_WALLPAPER_MODULE_DIR}"
    RUNTIME_OUTPUT_DIRECTORY "${QUICKSOV_WALLPAPER_MODULE_DIR}"
    BUILD_RPATH "$ORIGIN"
    INSTALL_RPATH "$ORIGIN"
)

target_include_directories(quicksov_wallpaper_mpv PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/src")

target_link_libraries(quicksov_wallpaper_mpv
    PRIVATE
        Qt::Core
        Qt::Gui
        Qt::Qml
        Qt::Quick
        Qt::OpenGL
        PkgConfig::MPV
)
