#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SOV710
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$REPO_ROOT/native/wallpaper_mpv"
BUILD_DIR="$REPO_ROOT/.build/native/wallpaper_mpv"
QML_OUT_DIR="$REPO_ROOT/.build/qml"
SHELL_LINK="$REPO_ROOT/shell/Quicksov"

mkdir -p "$BUILD_DIR" "$QML_OUT_DIR"

cmake -S "$SRC_DIR" -B "$BUILD_DIR" -G Ninja \
    -DCMAKE_BUILD_TYPE=RelWithDebInfo \
    -DQT_QML_OUTPUT_DIRECTORY="$QML_OUT_DIR"
cmake --build "$BUILD_DIR"

ln -sfn ../.build/qml/Quicksov "$SHELL_LINK"

echo "Built native wallpaper plugin into $QML_OUT_DIR"
