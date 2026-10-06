#!/usr/bin/env python3
"""Packaging verification script for WatchAI.

Validates Meson build configuration, Cargo build isolation, dual-location GSettings
schema installation, systemd unit generation, secure file permissions, and clean
uninstallation/reinstallation in an isolated temporary prefix.
"""

import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile


def check_static_packaging_assets(repo_root: str) -> bool:
    """Validate packaging definition files without requiring meson/ninja."""
    print("Running static packaging validation...")
    errors = []

    # 1. meson.build
    meson_build = os.path.join(repo_root, "meson.build")
    if not os.path.isfile(meson_build):
        errors.append("meson.build is missing")
    else:
        with open(meson_build, "r", encoding="utf-8") as f:
            content = f.read()
        if "project('watchai'" not in content and "project(\n  'watchai'" not in content:
            errors.append("meson.build missing project definition")
        if "watchai-daemon" not in content:
            errors.append("meson.build missing watchai-daemon target")
        if "watchai-mock" in content:
            errors.append("meson.build must NOT contain watchai-mock target")
        if "compile-extension-schemas" not in content:
            errors.append("meson.build missing local schema compilation target")

    # 2. cargo-build.py
    cargo_build = os.path.join(repo_root, "build-aux", "cargo-build.py")
    if not os.path.isfile(cargo_build):
        errors.append("build-aux/cargo-build.py is missing")
    elif not os.access(cargo_build, os.X_OK):
        errors.append("build-aux/cargo-build.py is not executable")
    else:
        with open(cargo_build, "r", encoding="utf-8") as f:
            cargo_content = f.read()
        if "--target-dir" not in cargo_content:
            errors.append("cargo-build.py does not enforce isolated --target-dir")

    # 3. systemd unit template
    service_in = os.path.join(repo_root, "systemd", "watchai.service.in")
    if not os.path.isfile(service_in):
        errors.append("systemd/watchai.service.in is missing")
    else:
        with open(service_in, "r", encoding="utf-8") as f:
            service_content = f.read()
        if "@bindir@/watchai-daemon" not in service_content:
            errors.append("systemd/watchai.service.in missing @bindir@ placeholder")

    # 4. metadata.json URL
    metadata_json = os.path.join(repo_root, "extension", "metadata.json")
    if not os.path.isfile(metadata_json):
        errors.append("extension/metadata.json is missing")
    else:
        try:
            with open(metadata_json, "r", encoding="utf-8") as f:
                data = json.load(f)
            if data.get("url") != "https://github.com/HA-ir/WatchAI":
                errors.append(f"metadata.json has unexpected URL: {data.get('url')}")
        except Exception as e:
            errors.append(f"Failed to parse metadata.json: {e}")

    # 5. Schema XML validation
    schema_dir = os.path.join(repo_root, "extension", "schemas")
    schema_xml = os.path.join(
        schema_dir, "org.gnome.shell.extensions.watchai.gschema.xml"
    )
    if not os.path.isfile(schema_xml):
        errors.append("GSettings schema XML is missing")
    elif shutil.which("glib-compile-schemas"):
        res = subprocess.run(
            ["glib-compile-schemas", "--strict", "--dry-run", schema_dir],
            capture_output=True,
            text=True,
        )
        if res.returncode != 0:
            errors.append(f"Schema dry-run validation failed: {res.stderr}")

    if errors:
        for err in errors:
            print(f"  FAILED: {err}", file=sys.stderr)
        return False

    print("  SUCCESS: All static packaging definitions validated successfully.")
    return True


def run_full_packaging_verification(repo_root: str) -> bool:
    """Run full meson setup, build, install, permission check, uninstall, and reinstall."""
    ninja_bin = shutil.which("ninja") or shutil.which("ninja-build")
    meson_bin = shutil.which("meson")

    if not meson_bin or not ninja_bin:
        print("\nNote: 'meson' or 'ninja' not found in PATH.")
        print("To run full packaging verification, install them via:")
        print("  sudo apt install meson ninja-build")
        print("Falling back to static packaging asset verification.")
        return check_static_packaging_assets(repo_root)

    # Static checks first
    if not check_static_packaging_assets(repo_root):
        return False

    print(f"\nRunning dynamic packaging verification with {meson_bin} and {ninja_bin}...")
    with tempfile.TemporaryDirectory(prefix="watchai-pkg-") as tmpdir:
        build_dir = os.path.join(tmpdir, "build")
        prefix_dir = os.path.join(tmpdir, "prefix")

        # 1. Meson setup
        print(f"Step 1: meson setup {build_dir} --prefix={prefix_dir}")
        setup_cmd = [
            meson_bin,
            "setup",
            build_dir,
            f"--prefix={prefix_dir}",
            repo_root,
        ]
        res = subprocess.run(setup_cmd, capture_output=True, text=True)
        if res.returncode != 0:
            print(f"meson setup failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}", file=sys.stderr)
            return False

        # 2. Build via Ninja
        print(f"Step 2: {ninja_bin} -C {build_dir}")
        res = subprocess.run([ninja_bin, "-C", build_dir], capture_output=True, text=True)
        if res.returncode != 0:
            print(f"ninja build failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}", file=sys.stderr)
            return False

        # Verify Cargo target directory isolation: <build_dir>/cargo-target must exist
        cargo_target = os.path.join(build_dir, "cargo-target")
        if not os.path.isdir(cargo_target):
            print(f"Error: Isolated cargo-target was not found at {cargo_target}", file=sys.stderr)
            return False

        # 3. Install via Ninja
        print(f"Step 3: {ninja_bin} -C {build_dir} install")
        res = subprocess.run([ninja_bin, "-C", build_dir, "install"], capture_output=True, text=True)
        if res.returncode != 0:
            print(f"ninja install failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}", file=sys.stderr)
            return False

        # 4. Assert installed artifacts & permissions
        print("Step 4: Inspecting installed artifacts and security permissions...")
        daemon_bin = os.path.join(prefix_dir, "bin", "watchai-daemon")
        if not os.path.isfile(daemon_bin):
            print(f"Error: Installed daemon binary missing at {daemon_bin}", file=sys.stderr)
            return False
        if not os.access(daemon_bin, os.X_OK):
            print(f"Error: Installed daemon binary is not executable", file=sys.stderr)
            return False

        daemon_mode = stat.S_IMODE(os.stat(daemon_bin).st_mode)
        if daemon_mode != 0o755:
            print(f"Warning: Installed daemon mode is {oct(daemon_mode)}, expected 0o755")

        # Assert watchai-mock is strictly absent
        mock_bin = os.path.join(prefix_dir, "bin", "watchai-mock")
        if os.path.exists(mock_bin):
            print(f"Error: watchai-mock found in install prefix! Must be excluded.", file=sys.stderr)
            return False

        # Extension files
        ext_dir = os.path.join(
            prefix_dir,
            "share",
            "gnome-shell",
            "extensions",
            "watchai@gnome.org",
        )
        for fname in ["extension.js", "indicator.js", "popover.js", "metadata.json", "stylesheet.css"]:
            fpath = os.path.join(ext_dir, fname)
            if not os.path.isfile(fpath):
                print(f"Error: Extension file missing: {fpath}", file=sys.stderr)
                return False
            fmode = stat.S_IMODE(os.stat(fpath).st_mode)
            if fmode & 0o022 != 0:
                print(f"Error: Asset {fpath} is group/world writable: {oct(fmode)}", file=sys.stderr)
                return False

        # Extension local schemas
        local_compiled = os.path.join(ext_dir, "schemas", "gschemas.compiled")
        if not os.path.isfile(local_compiled):
            print(f"Error: Extension local gschemas.compiled missing: {local_compiled}", file=sys.stderr)
            return False

        # System schemas
        sys_xml = os.path.join(
            prefix_dir,
            "share",
            "glib-2.0",
            "schemas",
            "org.gnome.shell.extensions.watchai.gschema.xml",
        )
        if not os.path.isfile(sys_xml):
            print(f"Error: System schema XML missing: {sys_xml}", file=sys.stderr)
            return False

        # Systemd service unit
        service_file = os.path.join(
            prefix_dir,
            "share",
            "systemd",
            "user",
            "watchai.service",
        )
        if not os.path.isfile(service_file):
            print(f"Error: systemd service unit missing: {service_file}", file=sys.stderr)
            return False

        with open(service_file, "r", encoding="utf-8") as f:
            service_body = f.read()
        expected_exec = f"ExecStart={prefix_dir}/bin/watchai-daemon"
        if expected_exec not in service_body:
            print(f"Error: Service unit missing expected ExecStart: {expected_exec}\nGot:\n{service_body}", file=sys.stderr)
            return False

        # 5. Clean uninstallation
        print(f"Step 5: {ninja_bin} -C {build_dir} uninstall")
        res = subprocess.run([ninja_bin, "-C", build_dir, "uninstall"], capture_output=True, text=True)
        if res.returncode != 0:
            print(f"ninja uninstall failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}", file=sys.stderr)
            return False

        # Verify removal
        if os.path.exists(daemon_bin):
            print(f"Error: Daemon binary was not removed by uninstall: {daemon_bin}", file=sys.stderr)
            return False
        if os.path.exists(local_compiled):
            print(f"Error: Local gschemas.compiled was not removed by uninstall: {local_compiled}", file=sys.stderr)
            return False
        if os.path.exists(service_file):
            print(f"Error: Service file was not removed by uninstall: {service_file}", file=sys.stderr)
            return False

        # 6. Reinstallation idempotency
        print(f"Step 6: {ninja_bin} -C {build_dir} install (reinstallation idempotency)")
        res = subprocess.run([ninja_bin, "-C", build_dir, "install"], capture_output=True, text=True)
        if res.returncode != 0:
            print(f"ninja reinstall failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}", file=sys.stderr)
            return False

        if not os.path.isfile(daemon_bin):
            print(f"Error: Reinstall failed to materialize daemon binary: {daemon_bin}", file=sys.stderr)
            return False

        print("\nAll dynamic packaging checks PASSED successfully!")
        return True


def main():
    repo_root = os.path.abspath(
        os.path.join(os.path.dirname(__file__), "..", "..")
    )
    success = run_full_packaging_verification(repo_root)
    if not success:
        sys.exit(1)
    print("\nPackaging verification completed successfully.")
    sys.exit(0)


if __name__ == "__main__":
    main()
