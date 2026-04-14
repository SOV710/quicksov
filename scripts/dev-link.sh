#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SOV710
# SPDX-License-Identifier: GPL-3.0-or-later
# Symlinks shell/ contents into ~/.config/quicksov/ for development.
# Each item in shell/ is individually linked so qs hot-reloads on save.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHELL_DIR="$REPO_ROOT/shell"
CONFIG_DIR="$HOME/.config/quicksov"

mkdir -p "$CONFIG_DIR"

link_item() {
    local src="$1"
    local dst="$CONFIG_DIR/$(basename "$src")"
    if [[ -L "$dst" ]]; then
        rm "$dst"
    elif [[ -e "$dst" ]]; then
        echo "warn: $dst exists and is not a symlink — skipping" >&2
        return
    fi
    ln -s "$src" "$dst"
    echo "  $dst -> $src"
}

echo "Linking shell/ into $CONFIG_DIR ..."
for item in "$SHELL_DIR"/*; do
    link_item "$item"
done

# Also link config templates (skip if real config already exists)
for f in daemon.toml.example design-tokens.toml; do
    src="$REPO_ROOT/config/$f"
    [[ -f "$src" ]] || continue
    dst_name="${f%.example}"
    dst="$CONFIG_DIR/$dst_name"
    if [[ ! -e "$dst" ]]; then
        ln -s "$src" "$dst"
        echo "  $dst -> $src"
    fi
done

# Link icons/ if present
if [[ -d "$REPO_ROOT/icons" ]]; then
    link_item "$REPO_ROOT/icons"
fi

# Link scripts/ so ipc-bridge is reachable at relative path from shell/ipc/
SCRIPTS_DIR="$REPO_ROOT/scripts"
dst="$CONFIG_DIR/scripts"
if [[ -L "$dst" ]]; then rm "$dst"; fi
if [[ ! -e "$dst" ]]; then
    ln -s "$SCRIPTS_DIR" "$dst"
    echo "  $dst -> $SCRIPTS_DIR"
fi

echo "Done. Start daemon: cargo run --manifest-path $REPO_ROOT/Cargo.toml"
echo "Start shell: QS_BASE_PATH=$CONFIG_DIR quickshell"
