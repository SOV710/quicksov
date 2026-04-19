#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SOV710
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$REPO_ROOT/.build/qml/Quicksov"
DEST="$HOME/.config/quickshell/quicksov/Quicksov"

if [[ ! -d "$SRC" ]]; then
    echo "error: plugin output missing; run scripts/build-wallpaper-plugin.sh first" >&2
    exit 1
fi

mkdir -p "$DEST"
rsync -a --delete "$SRC/" "$DEST/"
echo "Installed native wallpaper plugin to $DEST"
