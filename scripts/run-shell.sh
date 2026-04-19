#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SOV710
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export QSG_RHI_BACKEND=opengl
export LC_NUMERIC=C
export QML_IMPORT_PATH="$REPO_ROOT/.build/qml${QML_IMPORT_PATH:+:$QML_IMPORT_PATH}"
exec qs -c quicksov
