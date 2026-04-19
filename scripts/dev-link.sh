#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SOV710
# SPDX-License-Identifier: GPL-3.0-or-later
# Symlinks shell/ contents into ~/.config/quickshell/quicksov/ for development.
# Each item in shell/ is individually linked so qs hot-reloads on save.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHELL_DIR="$REPO_ROOT/shell"
CONFIG_DIR="$HOME/.config/quickshell/quicksov"

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
    if [[ "$(basename "$item")" == "Quicksov" ]]; then
        continue
    fi
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

PLUGIN_SRC="$REPO_ROOT/.build/qml/Quicksov"
PLUGIN_DST="$CONFIG_DIR/Quicksov"
if [[ -d "$PLUGIN_SRC" ]]; then
    if [[ -L "$PLUGIN_DST" ]]; then
        rm "$PLUGIN_DST"
    elif [[ -e "$PLUGIN_DST" ]]; then
        echo "warn: $PLUGIN_DST exists and is not a symlink — skipping plugin link" >&2
    else
        ln -s "$PLUGIN_SRC" "$PLUGIN_DST"
        echo "  $PLUGIN_DST -> $PLUGIN_SRC"
    fi
else
    echo "warn: native wallpaper plugin not built yet; run scripts/build-wallpaper-plugin.sh" >&2
fi

echo "Done. Start daemon: cargo run --manifest-path $REPO_ROOT/Cargo.toml"
echo "Start shell: $REPO_ROOT/scripts/run-shell.sh"
