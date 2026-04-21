#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SOV710
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$REPO_ROOT/cpp/wallpaper/renderer"
BUILD_DIR="$REPO_ROOT/.build/cpp/wallpaper/renderer"

mkdir -p "$BUILD_DIR"

cmake -S "$SRC_DIR" -B "$BUILD_DIR" -G Ninja \
    -DCMAKE_BUILD_TYPE=RelWithDebInfo
cmake --build "$BUILD_DIR"

echo "Built wallpaper renderer into $BUILD_DIR/qsov-wallpaper-renderer"
