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

if [[ -d "$REPO_ROOT/icons" ]]; then
    rsync -a --delete "$REPO_ROOT/icons/" "$DEST/icons/"
    echo "Installed icons/"
fi

echo "Done. Run shell: quickshell --config quicksov"
