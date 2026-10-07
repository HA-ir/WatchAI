#!/usr/bin/env bash
# WatchAI Uninstaller Shortcut
# Forwards execution to ./install.sh --uninstall

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/install.sh" --uninstall "$@"
