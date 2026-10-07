#!/usr/bin/env bash
# Automated test suite for install.sh and uninstall.sh
# Tests help, dry-run, prefix isolation, artifact installation, GSettings verification,
# idempotency, and clean uninstallation in an isolated temporary directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

INSTALL_SH="$REPO_ROOT/install.sh"
UNINSTALL_SH="$REPO_ROOT/uninstall.sh"

TEST_TMPDIR=$(mktemp -d -t watchai-installer-test-XXXXXX)
TEST_PREFIX="$TEST_TMPDIR/prefix"
TEST_BUILDDIR="$TEST_TMPDIR/build"

cleanup() {
    rm -rf "$TEST_TMPDIR"
}
trap cleanup EXIT

echo "=== Running Installer Tests in Isolated Environment: $TEST_TMPDIR ==="

# Test 1: Help flags
echo "[Test 1] Verify --help on install.sh and uninstall.sh..."
"$INSTALL_SH" --help >/dev/null
"$UNINSTALL_SH" --help >/dev/null
echo "  PASS: Help flags functional."

# Test 2: Dry-run flag
echo "[Test 2] Verify --dry-run mode..."
dry_run_output=$("$INSTALL_SH" --dry-run --prefix "$TEST_PREFIX" --build-dir "$TEST_BUILDDIR")
if ! echo "$dry_run_output" | grep -q "Dry run complete"; then
    echo "  FAIL: Dry-run did not output expected success confirmation." >&2
    exit 1
fi
if [[ -d "$TEST_PREFIX" ]]; then
    echo "  FAIL: Dry-run created target prefix unexpectedly." >&2
    exit 1
fi
echo "  PASS: Dry-run mode verified."

# Test 3: Isolated Installation with --no-start
echo "[Test 3] Execute isolated build and install with --no-start..."
"$INSTALL_SH" --no-start --prefix "$TEST_PREFIX" --build-dir "$TEST_BUILDDIR"

# Verify installed artifacts
if [[ ! -x "$TEST_PREFIX/bin/watchai-daemon" ]]; then
    echo "  FAIL: Daemon binary missing or not executable at $TEST_PREFIX/bin/watchai-daemon" >&2
    exit 1
fi
if [[ ! -f "$TEST_PREFIX/share/systemd/user/watchai.service" ]]; then
    echo "  FAIL: Service file missing at $TEST_PREFIX/share/systemd/user/watchai.service" >&2
    exit 1
fi
if [[ ! -f "$TEST_PREFIX/share/gnome-shell/extensions/watchai@gnome.org/metadata.json" ]]; then
    echo "  FAIL: Extension metadata missing" >&2
    exit 1
fi
if [[ ! -f "$TEST_PREFIX/share/gnome-shell/extensions/watchai@gnome.org/schemas/gschemas.compiled" ]]; then
    echo "  FAIL: Extension compiled schema missing" >&2
    exit 1
fi
if [[ ! -f "$TEST_PREFIX/share/glib-2.0/schemas/gschemas.compiled" ]]; then
    echo "  FAIL: System compiled schema missing" >&2
    exit 1
fi

# Verify GSettings keys in installed prefix
ext_schemas="$TEST_PREFIX/share/gnome-shell/extensions/watchai@gnome.org/schemas"
keys=$(gsettings --schemadir "$ext_schemas" list-keys org.gnome.shell.extensions.watchai)
expected_keys=("dwell-duration-seconds" "enable-desktop-notifications" "notify-on-waiting" "notify-on-error" "indicator-icon-style")
for k in "${expected_keys[@]}"; do
    if ! echo "$keys" | grep -qx "$k"; then
        echo "  FAIL: Key '$k' missing from installed schemas in $ext_schemas" >&2
        exit 1
    fi
done
echo "  PASS: Isolated installation and all 5 GSettings keys verified."

# Test 4: Idempotency (run installer second time over existing build and prefix)
echo "[Test 4] Verify reinstallation idempotency..."
"$INSTALL_SH" --no-start --prefix "$TEST_PREFIX" --build-dir "$TEST_BUILDDIR"
if [[ ! -x "$TEST_PREFIX/bin/watchai-daemon" ]]; then
    echo "  FAIL: Reinstallation failed to materialize daemon binary." >&2
    exit 1
fi
echo "  PASS: Idempotent reinstallation verified."

# Test 5: Clean Uninstallation via --uninstall
echo "[Test 5] Verify clean uninstallation..."
"$INSTALL_SH" --uninstall --prefix "$TEST_PREFIX" --build-dir "$TEST_BUILDDIR"

# Verify all artifacts removed
if [[ -f "$TEST_PREFIX/bin/watchai-daemon" ]]; then
    echo "  FAIL: Daemon binary was not removed." >&2
    exit 1
fi
if [[ -f "$TEST_PREFIX/share/systemd/user/watchai.service" ]]; then
    echo "  FAIL: Service file was not removed." >&2
    exit 1
fi
if [[ -d "$TEST_PREFIX/share/gnome-shell/extensions/watchai@gnome.org" ]]; then
    echo "  FAIL: Extension directory was not removed." >&2
    exit 1
fi
if [[ -f "$TEST_PREFIX/share/glib-2.0/schemas/org.gnome.shell.extensions.watchai.gschema.xml" ]]; then
    echo "  FAIL: System schema XML was not removed." >&2
    exit 1
fi
if [[ -f "$TEST_PREFIX/share/glib-2.0/schemas/gschemas.compiled" ]]; then
    echo "  FAIL: System compiled schema was not removed." >&2
    exit 1
fi
echo "  PASS: Clean uninstallation verified."

echo "=== All Installer Tests Passed Successfully! ==="
