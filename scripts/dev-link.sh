#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SOV710
# SPDX-License-Identifier: GPL-3.0-or-later
# Links shell/ into ~/.config/quickshell/quicksov for development.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHELL_DIR="$REPO_ROOT/shell"
LINK_TARGET="$HOME/.config/quickshell/quicksov"

if [[ -L "$LINK_TARGET" ]]; then
    echo "Removing existing symlink: $LINK_TARGET"
    rm "$LINK_TARGET"
elif [[ -e "$LINK_TARGET" ]]; then
    echo "Error: $LINK_TARGET exists and is not a symlink. Remove it manually." >&2
    exit 1
fi

mkdir -p "$(dirname "$LINK_TARGET")"
ln -s "$SHELL_DIR" "$LINK_TARGET"
echo "Linked: $LINK_TARGET -> $SHELL_DIR"
echo "Run quickshell with: quickshell -p quicksov"
