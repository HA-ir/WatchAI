#!/usr/bin/env python3
"""Packaging validation for the GNOME Shell extension zip bundle (EGO distribution).

Validates:
1. Root-level existence of metadata.json, extension.js, stylesheet.css, and all helper modules.
2. Structure of schemas/ containing both org.gnome.shell.extensions.watchai.gschema.xml and gschemas.compiled.
3. Absence of tests, temporary build files, or extraneous directories.
4. Correctness of metadata.json inside the zip bundle.
"""

import json
import os
import subprocess
import sys
import tempfile
import zipfile

REQUIRED_ROOT_FILES = {
    "metadata.json",
    "extension.js",
    "stylesheet.css",
    "dbus_client.js",
    "indicator.js",
    "notifications.js",
    "popover.js",
    "prefs.js",
    "settings.js",
    "utils.js",
}

REQUIRED_SCHEMA_FILES = {
    "schemas/org.gnome.shell.extensions.watchai.gschema.xml",
    "schemas/gschemas.compiled",
}

FORBIDDEN_PATTERNS = [
    "tests/",
    "test_",
    ".git",
    "__pycache__",
    ".DS_Store",
    "cargo-target",
    ".cargo",
]


def test_extension_zip_package():
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
    pack_script = os.path.join(repo_root, "scripts", "pack-extension.sh")

    with tempfile.TemporaryDirectory(prefix="watchai-ego-test-") as tmpdir:
        zip_path = os.path.join(tmpdir, "watchai@gnome.org.shell-extension.zip")

        print(f"Executing packaging script: {pack_script} ...")
        res = subprocess.run(
            [pack_script, repo_root, tmpdir],
            capture_output=True,
            text=True,
        )
        if res.returncode != 0:
            sys.stderr.write(f"pack-extension.sh failed:\n{res.stdout}\n{res.stderr}\n")
            sys.exit(1)

        if not os.path.isfile(zip_path):
            sys.stderr.write(f"Target zip not created at {zip_path}\n")
            sys.exit(1)

        print(f"Inspecting zip bundle: {zip_path}")
        with zipfile.ZipFile(zip_path, "r") as zf:
            namelist = set(zf.namelist())

            # Filter out directory entries like 'schemas/'
            file_entries = {n for n in namelist if not n.endswith("/")}

            # 1. Verify root files
            for req in REQUIRED_ROOT_FILES:
                if req not in file_entries:
                    sys.stderr.write(f"Missing required root file in zip: {req}\n")
                    sys.exit(1)

            # 2. Verify schema files
            for req in REQUIRED_SCHEMA_FILES:
                if req not in file_entries:
                    sys.stderr.write(f"Missing required schema file in zip: {req}\n")
                    sys.exit(1)

            # 3. Verify forbidden files
            for entry in namelist:
                for forbidden in FORBIDDEN_PATTERNS:
                    if forbidden in entry:
                        sys.stderr.write(
                            f"Forbidden file pattern '{forbidden}' found in zip: {entry}\n"
                        )
                        sys.exit(1)

            # 4. Verify metadata.json content
            with zf.open("metadata.json") as meta_f:
                meta = json.load(meta_f)
                if meta.get("uuid") != "watchai@gnome.org":
                    sys.stderr.write(f"Unexpected uuid in metadata: {meta.get('uuid')}\n")
                    sys.exit(1)
                if not meta.get("shell-version"):
                    sys.stderr.write("Missing shell-version in metadata.json\n")
                    sys.exit(1)
                if meta.get("url") != "https://github.com/HA-ir/WatchAI":
                    sys.stderr.write(f"Unexpected url in metadata: {meta.get('url')}\n")
                    sys.exit(1)

        print("Extension zip package validation PASSED successfully.")


if __name__ == "__main__":
    test_extension_zip_package()
