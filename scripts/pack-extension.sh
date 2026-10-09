#!/usr/bin/env bash
# Pack GNOME Shell extension for extensions.gnome.org (EGO) distribution
#
# Generates: <output_dir>/watchai@gnome.org.shell-extension.zip
#
# Bundle Structure:
#   metadata.json
#   extension.js
#   stylesheet.css
#   dbus_client.js
#   indicator.js
#   notifications.js
#   popover.js
#   settings.js
#   utils.js
#   schemas/
#     org.gnome.shell.extensions.watchai.gschema.xml
#     gschemas.compiled

set -euo pipefail

REPO_ROOT="${1:-$(cd "$(dirname "$0")/.." && pwd)}"
OUT_DIR="${2:-$REPO_ROOT/build}"

EXT_DIR="$REPO_ROOT/extension"
SCHEMA_DIR="$EXT_DIR/schemas"
UUID="watchai@gnome.org"
ZIP_NAME="${UUID}.shell-extension.zip"

mkdir -p "$OUT_DIR"
TARGET_ZIP="$OUT_DIR/$ZIP_NAME"

echo "Packing WatchAI GNOME extension ($UUID)..."
echo "  Source directory: $EXT_DIR"
echo "  Output target:    $TARGET_ZIP"

# 1. Ensure schemas are compiled locally
if command -v glib-compile-schemas >/dev/null 2>&1; then
    echo "  Compiling GSettings schemas..."
    glib-compile-schemas "$SCHEMA_DIR"
else
    echo "WARNING: glib-compile-schemas not found. Ensuring schemas/gschemas.compiled exists." >&2
    if [ ! -f "$SCHEMA_DIR/gschemas.compiled" ]; then
        echo "ERROR: schemas/gschemas.compiled is missing and glib-compile-schemas is not in PATH." >&2
        exit 1
    fi
fi

# 2. Package extension bundle
if command -v gnome-extensions >/dev/null 2>&1; then
    echo "  Creating bundle via gnome-extensions pack..."
    TMP_PACK_DIR="$(mktemp -d -t watchai-pack-XXXXXX)"
    trap 'rm -rf "$TMP_PACK_DIR"' EXIT

    gnome-extensions pack "$EXT_DIR" \
        --force \
        --out-dir="$TMP_PACK_DIR" \
        --schema=schemas/org.gnome.shell.extensions.watchai.gschema.xml \
        --extra-source=dbus_client.js \
        --extra-source=indicator.js \
        --extra-source=notifications.js \
        --extra-source=popover.js \
        --extra-source=prefs.js \
        --extra-source=settings.js \
        --extra-source=utils.js

    # gnome-extensions pack includes the XML schema in schemas/, but not gschemas.compiled.
    # Append schemas/gschemas.compiled into schemas/ directory in the zip using python3:
    python3 -c "
import zipfile, os
zpath = os.path.join('$TMP_PACK_DIR', '$ZIP_NAME')
compiled_schema = os.path.join('$SCHEMA_DIR', 'gschemas.compiled')
with zipfile.ZipFile(zpath, 'a') as zf:
    zf.write(compiled_schema, 'schemas/gschemas.compiled')
"
    mv "$TMP_PACK_DIR/$ZIP_NAME" "$TARGET_ZIP"
else
    echo "  gnome-extensions CLI not found. Packaging bundle via Python zipfile..."
    python3 -c "
import zipfile, os

ext_dir = '$EXT_DIR'
target_zip = '$TARGET_ZIP'

files_to_pack = [
    ('metadata.json', 'metadata.json'),
    ('extension.js', 'extension.js'),
    ('stylesheet.css', 'stylesheet.css'),
    ('dbus_client.js', 'dbus_client.js'),
    ('indicator.js', 'indicator.js'),
    ('notifications.js', 'notifications.js'),
    ('popover.js', 'popover.js'),
    ('prefs.js', 'prefs.js'),
    ('settings.js', 'settings.js'),
    ('utils.js', 'utils.js'),
    ('schemas/org.gnome.shell.extensions.watchai.gschema.xml', 'schemas/org.gnome.shell.extensions.watchai.gschema.xml'),
    ('schemas/gschemas.compiled', 'schemas/gschemas.compiled'),
]

with zipfile.ZipFile(target_zip, 'w', compression=zipfile.ZIP_DEFLATED) as zf:
    for src, arc in files_to_pack:
        full_src = os.path.join(ext_dir, src)
        if not os.path.isfile(full_src):
            raise FileNotFoundError(f'Required file missing: {full_src}')
        zf.write(full_src, arc)
print(f'Successfully created {target_zip}')
"
fi

echo "  Extension package verified at: $TARGET_ZIP"
python3 -c "
import zipfile
with zipfile.ZipFile('$TARGET_ZIP', 'r') as zf:
    print('  Archive contents (' + str(len(zf.namelist())) + ' entries):')
    for info in zf.infolist():
        print(f'    - {info.filename} ({info.file_size} bytes)')
"
