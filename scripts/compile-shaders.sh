#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SOV710
# SPDX-License-Identifier: GPL-3.0-or-later
# Compiles Qt Quick shaders from shell/shaders/src into distributable qsb files.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$REPO_ROOT/shell/shaders/src"
OUT_DIR="$REPO_ROOT/shell/shaders/qsb"

QSB_BIN="${QSB_BIN:-}"
if [[ -z "$QSB_BIN" ]]; then
    for candidate in \
        "$REPO_ROOT/.qt/bin/qsb" \
        "/usr/lib/qt6/bin/qsb" \
        "/usr/lib64/qt6/bin/qsb" \
        "qsb"; do
        if command -v "$candidate" >/dev/null 2>&1; then
            QSB_BIN="$(command -v "$candidate")"
            break
        fi
    done
fi

if [[ -z "$QSB_BIN" ]]; then
    echo "error: qsb not found; install Qt Shader Tools / qtshadertools" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"

shopt -s nullglob
for shader in "$SRC_DIR"/*.frag; do
    out="$OUT_DIR/$(basename "$shader").qsb"
    "$QSB_BIN" --qt6 -o "$out" "$shader"
    echo "compiled ${shader#$REPO_ROOT/} -> ${out#$REPO_ROOT/}"
done
