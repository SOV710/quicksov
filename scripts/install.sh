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
rsync -a --delete --exclude=icons --exclude=Quicksov "$SHELL_SRC/" "$DEST/"

if [[ -d "$REPO_ROOT/icons" ]]; then
    rsync -a --delete "$REPO_ROOT/icons/" "$DEST/icons/"
    echo "Installed icons/"
fi

if [[ -d "$REPO_ROOT/.build/qml/Quicksov" ]]; then
    rsync -a --delete "$REPO_ROOT/.build/qml/Quicksov/" "$DEST/Quicksov/"
    echo "Installed Quicksov/ native plugin"
else
    echo "warn: native wallpaper plugin not built; skipping Quicksov/ install" >&2
fi

echo "Done. Run shell: $REPO_ROOT/scripts/run-shell.sh"
