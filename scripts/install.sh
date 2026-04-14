#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SOV710
# SPDX-License-Identifier: GPL-3.0-or-later
# Installs quicksov shell files to ~/.config/quickshell/quicksov.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHELL_SRC="$REPO_ROOT/shell"
DEST="$HOME/.config/quickshell/quicksov"

echo "Installing quicksov shell to $DEST"

mkdir -p "$DEST"
rsync -a --delete "$SHELL_SRC/" "$DEST/"

echo "Installing ipc-bridge to $DEST/../../../local/bin/quicksov-ipc-bridge"
BRIDGE_DEST="$HOME/.local/bin/quicksov-ipc-bridge"
mkdir -p "$(dirname "$BRIDGE_DEST")"
install -m 0755 "$REPO_ROOT/scripts/ipc-bridge" "$BRIDGE_DEST"

echo "Done. Run quickshell with: quickshell -p quicksov"
