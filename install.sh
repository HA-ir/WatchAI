#!/usr/bin/env bash
# WatchAI Local Installer & Lifecycle Manager
# Installs and manages the WatchAI daemon and GNOME Shell extension in the user space ($HOME/.local).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"

DEFAULT_PREFIX="${HOME}/.local"
PREFIX="${PREFIX:-$DEFAULT_PREFIX}"
BUILD_DIR="${BUILD_DIR:-$REPO_ROOT/build}"

EXTENSION_UUID="watchai@gnome.org"
SERVICE_NAME="watchai.service"
DBUS_NAME="org.freedesktop.WatchAI"
DBUS_PATH="/org/freedesktop/WatchAI"
SCHEMA_ID="org.gnome.shell.extensions.watchai"

# Formatting helpers
info() { echo -e "\033[1;34m[INFO]\033[0m $*"; }
success() { echo -e "\033[1;32m[SUCCESS]\033[0m $*"; }
warn() { echo -e "\033[1;33m[WARN]\033[0m $*"; }
error() { echo -e "\033[1;31m[ERROR]\033[0m $*" >&2; }

show_help() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Production-quality local installer and lifecycle manager for WatchAI.
Installs the daemon binary, systemd user service, GSettings schemas, and
GNOME Shell extension into the user directory without requiring root.

Options:
  --help, -h          Show this help message and exit
  --uninstall, -u     Cleanly uninstall WatchAI, stop user service, and remove assets
  --pack              Pack GNOME Shell extension into .shell-extension.zip (for extensions.gnome.org)
  --dry-run           Check prerequisites and show installation plan without modifying system
  --no-start          Install artifacts but do not start systemd service or enable extension
  --with-claude-hooks Register live activity telemetry hooks in ~/.claude/settings.json
  --prefix DIR        Set target installation prefix (default: \$HOME/.local)
  --build-dir DIR     Set Meson build directory (default: <repo>/build)

Environment Variables:
  PREFIX              Override default install prefix (default: \$HOME/.local)
  BUILD_DIR           Override default build directory (default: <repo>/build)
EOF
}

check_prerequisites() {
    local missing=0

    # Essential build and toolchain requirements
    local required_commands=(
        "meson:Meson build system (install via apt install meson or dnf install meson)"
        "ninja:Ninja build tool (install via apt install ninja-build or dnf install ninja-build)"
        "cargo:Rust package manager & toolchain (install via https://rustup.rs)"
        "rustc:Rust compiler (install via https://rustup.rs)"
        "glib-compile-schemas:GLib schema compiler (install via libglib2.0-bin or glib2-devel)"
        "gsettings:GSettings command-line tool (install via libglib2.0-bin or glib2)"
        "systemctl:systemd service manager"
    )

    for item in "${required_commands[@]}"; do
        local cmd="${item%%:*}"
        local desc="${item#*:}"
        if ! command -v "$cmd" >/dev/null 2>&1; then
            error "Missing prerequisite: $cmd — $desc"
            missing=1
        fi
    done

    # Optional desktop utilities (warn if missing, don't fail immediately)
    if ! command -v gnome-extensions >/dev/null 2>&1; then
        warn "gnome-extensions tool not found; GNOME extension enabling will be skipped."
    fi

    if ! command -v busctl >/dev/null 2>&1; then
        warn "busctl tool not found; D-Bus IPC verification will be limited."
    fi

    if [[ "$missing" -ne 0 ]]; then
        error "Please install all missing build prerequisites and rerun $(basename "$0")."
        exit 1
    fi
}

do_uninstall() {
    info "Initiating clean uninstallation of WatchAI from prefix: $PREFIX"

    # 1. Stop and disable systemd user service if active
    if command -v systemctl >/dev/null 2>&1; then
        if systemctl --user is-active "$SERVICE_NAME" >/dev/null 2>&1; then
            info "Stopping systemd user service '$SERVICE_NAME'..."
            systemctl --user stop "$SERVICE_NAME" || warn "Failed to stop $SERVICE_NAME"
        fi
        if systemctl --user is-enabled "$SERVICE_NAME" >/dev/null 2>&1; then
            info "Disabling systemd user service '$SERVICE_NAME'..."
            systemctl --user disable "$SERVICE_NAME" || warn "Failed to disable $SERVICE_NAME"
        fi
    fi

    # 2. Disable GNOME extension if enabled
    if command -v gnome-extensions >/dev/null 2>&1; then
        if gnome-extensions list --enabled 2>/dev/null | grep -qx "$EXTENSION_UUID"; then
            info "Disabling GNOME Shell extension '$EXTENSION_UUID'..."
            gnome-extensions disable "$EXTENSION_UUID" || warn "Could not disable extension via CLI."
        fi
    fi

    # 3. Use Meson's tracked uninstall if build directory exists and is configured
    if [[ -d "$BUILD_DIR" ]] && [[ -f "$BUILD_DIR/build.ninja" ]]; then
        info "Running Ninja uninstall in $BUILD_DIR..."
        ninja -C "$BUILD_DIR" uninstall || warn "Ninja uninstall encountered warnings."
    else
        info "Build directory not configured with Ninja; removing known installed files manually from $PREFIX..."
        rm -f "$PREFIX/bin/watchai-daemon"
        rm -f "$PREFIX/share/systemd/user/$SERVICE_NAME"
        rm -f "$PREFIX/share/glib-2.0/schemas/org.gnome.shell.extensions.watchai.gschema.xml"
        rm -f "$PREFIX/share/glib-2.0/schemas/gschemas.compiled"
        rm -rf "$PREFIX/share/gnome-shell/extensions/$EXTENSION_UUID"
    fi

    # Ensure extension folder is completely purged from target prefix
    rm -rf "$PREFIX/share/gnome-shell/extensions/$EXTENSION_UUID"

    # 4. Reload systemd user daemon
    if command -v systemctl >/dev/null 2>&1; then
        info "Reloading systemd user daemon..."
        systemctl --user daemon-reload || true
    fi

    success "WatchAI has been completely and cleanly uninstalled from $PREFIX."
    echo "Note: Persistent user preferences in dconf are preserved per GNOME conventions."
}

main() {
    local action="install"
    local dry_run=0
    local no_start=0
    local with_claude_hooks=0

    while [[ $# -gt 0 ]]; do
        case "$1" in
            -h|--help)
                show_help
                exit 0
                ;;
            -u|--uninstall)
                action="uninstall"
                shift
                ;;
            --pack)
                action="pack"
                shift
                ;;
            --dry-run)
                dry_run=1
                shift
                ;;
            --no-start)
                no_start=1
                shift
                ;;
            --with-claude-hooks)
                with_claude_hooks=1
                shift
                ;;
            --prefix)
                PREFIX="$2"
                shift 2
                ;;
            --build-dir)
                BUILD_DIR="$2"
                shift 2
                ;;
            *)
                error "Unknown argument: $1"
                show_help
                exit 1
                ;;
        esac
    done

    # Handle uninstallation
    if [[ "$action" == "uninstall" ]]; then
        do_uninstall
        exit 0
    fi

    # Handle packaging
    if [[ "$action" == "pack" ]]; then
        info "Packing GNOME Shell extension for distribution..."
        "$REPO_ROOT/scripts/pack-extension.sh" "$REPO_ROOT" "$BUILD_DIR"
        exit 0
    fi

    info "Starting WatchAI local installation..."
    info "Repository root: $REPO_ROOT"
    info "Installation prefix: $PREFIX"
    info "Build directory: $BUILD_DIR"

    # 1. Prerequisite checks
    check_prerequisites

    if [[ "$dry_run" -eq 1 ]]; then
        info "Dry run enabled. The following steps would be performed:"
        echo "  1. meson setup --prefix=$PREFIX $BUILD_DIR $REPO_ROOT"
        echo "  2. ninja -C $BUILD_DIR"
        echo "  3. ninja -C $BUILD_DIR install"
        echo "  4. systemctl --user daemon-reload"
        echo "  5. systemctl --user enable --now $SERVICE_NAME"
        echo "  6. gnome-extensions enable $EXTENSION_UUID"
        echo "  7. Post-install health verification (D-Bus, GSettings, process)"
        success "Dry run complete: All prerequisites present, installation plan ready."
        exit 0
    fi

    # 2. Configure Meson build (reuse or reconfigure existing build)
    if [[ ! -d "$BUILD_DIR" ]]; then
        info "Configuring fresh Meson build directory..."
        meson setup --prefix="$PREFIX" "$BUILD_DIR" "$REPO_ROOT"
    elif [[ ! -f "$BUILD_DIR/build.ninja" ]]; then
        info "Reconfiguring existing empty/unconfigured build directory..."
        meson setup --prefix="$PREFIX" --reconfigure "$BUILD_DIR" "$REPO_ROOT"
    else
        info "Reusing existing Meson build directory in $BUILD_DIR (updating configuration)..."
        meson setup --prefix="$PREFIX" --reconfigure "$BUILD_DIR" "$REPO_ROOT" || {
            warn "Reconfigure failed; wiping and re-initializing build directory..."
            rm -rf "$BUILD_DIR"
            meson setup --prefix="$PREFIX" "$BUILD_DIR" "$REPO_ROOT"
        }
    fi

    # 3. Build components
    info "Compiling WatchAI components via Ninja and Cargo..."
    ninja -C "$BUILD_DIR"

    # 4. Install components
    info "Installing WatchAI into prefix: $PREFIX..."
    ninja -C "$BUILD_DIR" install

    # 5. Verify local installed files
    info "Verifying installed files..."
    local daemon_bin="$PREFIX/bin/watchai-daemon"
    local service_unit="$PREFIX/share/systemd/user/$SERVICE_NAME"
    local ext_dir="$PREFIX/share/gnome-shell/extensions/$EXTENSION_UUID"
    local ext_compiled_schema="$ext_dir/schemas/gschemas.compiled"

    if [[ ! -x "$daemon_bin" ]]; then
        error "Installed daemon binary missing or not executable at: $daemon_bin"
        exit 1
    fi

    if [[ ! -f "$service_unit" ]]; then
        error "Installed systemd user service unit missing at: $service_unit"
        exit 1
    fi

    if [[ ! -f "$ext_compiled_schema" ]]; then
        error "Installed extension compiled schema missing at: $ext_compiled_schema"
        exit 1
    fi

    # 6. Verify GSettings schemas in target prefix
    info "Verifying GSettings schema keys in installed prefix..."
    local gsettings_keys
    gsettings_keys=$(gsettings --schemadir "$ext_dir/schemas" list-keys "$SCHEMA_ID" 2>/dev/null || true)
    local expected_keys=("dwell-duration-seconds" "enable-desktop-notifications" "notify-on-waiting" "notify-on-error" "indicator-icon-style")
    for key in "${expected_keys[@]}"; do
        if ! echo "$gsettings_keys" | grep -qx "$key"; then
            error "GSettings key '$key' missing from installed schema database at $ext_dir/schemas"
            exit 1
        fi
    done
    success "All 5 GSettings schema keys validated successfully in extension directory."

    if [[ "$no_start" -eq 1 ]]; then
        success "Installation completed (--no-start specified). Artifacts installed at $PREFIX."
        exit 0
    fi

    # 7. Reload and restart systemd user service
    info "Reloading systemd user manager..."
    systemctl --user daemon-reload

    info "Enabling and starting $SERVICE_NAME..."
    systemctl --user enable "$SERVICE_NAME"
    systemctl --user restart "$SERVICE_NAME"

    # Wait briefly for service and D-Bus readiness
    sleep 1.5

    # Check systemd service status
    if systemctl --user is-active "$SERVICE_NAME" >/dev/null 2>&1; then
        success "Daemon service is active and running!"
    else
        error "Daemon service failed to start. Service status:"
        systemctl --user status "$SERVICE_NAME" --no-pager || true
        error "Recent journal logs:"
        journalctl --user -u "$SERVICE_NAME" -n 25 --no-pager || true
        exit 1
    fi

    # 8. Check D-Bus availability
    if command -v busctl >/dev/null 2>&1; then
        info "Verifying D-Bus service availability on session bus..."
        local dbus_ready=0
        for _ in {1..5}; do
            if busctl --user status "$DBUS_NAME" >/dev/null 2>&1; then
                dbus_ready=1
                break
            fi
            sleep 0.5
        done

        if [[ "$dbus_ready" -eq 1 ]]; then
            success "D-Bus service '$DBUS_NAME' is active and reachable!"
        else
            warn "Daemon is running, but D-Bus bus name '$DBUS_NAME' was not claimed yet."
        fi
    fi

    # 9. Enable GNOME Shell extension
    if command -v gnome-extensions >/dev/null 2>&1; then
        info "Enabling GNOME Shell extension '$EXTENSION_UUID'..."
        if gnome-extensions enable "$EXTENSION_UUID" 2>/dev/null; then
            success "GNOME Shell extension enabled successfully!"
        else
            warn "Could not enable GNOME extension via CLI (this is normal if no graphical GNOME session is currently focused)."
            echo "  You can enable it manually in your desktop session via:"
            echo "    gnome-extensions enable $EXTENSION_UUID"
        fi
    else
        warn "gnome-extensions CLI not available; please enable $EXTENSION_UUID via the Extensions app."
    fi

    # 10. Optional Claude Code activity telemetry hooks
    if [[ "$with_claude_hooks" -eq 1 ]]; then
        info "Configuring Claude Code live activity telemetry hooks..."
        "$daemon_bin" install-hooks || warn "Could not automatically register Claude Code hooks."
    fi

    # 11. Final success summary
    echo
    echo "=========================================================="
    success "WatchAI has been successfully installed and activated!"
    echo "=========================================================="
    echo "• Daemon binary:       $daemon_bin"
    echo "• Systemd service:     $service_unit"
    echo "• GNOME extension:     $ext_dir"
    echo "• Service status:      systemctl --user status $SERVICE_NAME"
    echo "• View daemon logs:    journalctl --user -u $SERVICE_NAME -f"
    echo "• Inspect D-Bus:       busctl --user introspect $DBUS_NAME $DBUS_PATH"
    echo "• Claude live hooks:   $daemon_bin status-hooks (run '$daemon_bin install-hooks' to enable)"
    echo "• Uninstall anytime:   $0 --uninstall"
    echo
    if [[ ":$PATH:" != *":$PREFIX/bin:"* ]]; then
        echo "Notice: To invoke watchai-daemon manually from terminal, add to PATH:"
        echo "  export PATH=\"$PREFIX/bin:\$PATH\""
        echo
    fi
}

main "$@"
