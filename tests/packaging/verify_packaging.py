#!/usr/bin/env python3
"""Packaging verification script for WatchAI.

Validates Meson build configuration, Cargo build isolation, dual-location GSettings
schema installation, systemd unit generation, secure file permissions, and clean
uninstallation/reinstallation in an isolated temporary prefix.
"""

import argparse
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile


def check_static_packaging_assets(repo_root: str) -> bool:
    """Validate packaging definition files statically."""
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
        if "subdir('data')" not in content:
            errors.append("meson.build missing data subdirectory inclusion for system schema tracking")

    # 2. data/meson.build
    data_meson = os.path.join(repo_root, "data", "meson.build")
    if not os.path.isfile(data_meson):
        errors.append("data/meson.build is missing")
    else:
        with open(data_meson, "r", encoding="utf-8") as f:
            data_content = f.read()
        if "compile-system-schemas" not in data_content:
            errors.append("data/meson.build missing compile-system-schemas target")

    # 3. cargo-build.py
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

    # 4. systemd unit template
    service_in = os.path.join(repo_root, "systemd", "watchai.service.in")
    if not os.path.isfile(service_in):
        errors.append("systemd/watchai.service.in is missing")
    else:
        with open(service_in, "r", encoding="utf-8") as f:
            service_content = f.read()
        if "@bindir@/watchai-daemon" not in service_content:
            errors.append("systemd/watchai.service.in missing @bindir@ placeholder")

    # 5. metadata.json URL
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

    # 6. Schema XML validation
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
    """Run full meson setup, build, install, permission check, uninstall, and reinstall.

    Fails if meson or ninja is not found in PATH.
    """
    ninja_bin = shutil.which("ninja") or shutil.which("ninja-build")
    meson_bin = shutil.which("meson")

    if not meson_bin or not ninja_bin:
        missing = []
        if not meson_bin:
            missing.append("meson")
        if not ninja_bin:
            missing.append("ninja (or ninja-build)")
        sys.stderr.write(
            f"Error: Required build tools missing from PATH: {', '.join(missing)}\n"
            "Full packaging verification is mandatory and cannot execute without them.\n"
            "Install prerequisites via:\n"
            "  sudo apt install meson ninja-build\n"
        )
        return False

    # Perform static checks first
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
            sys.stderr.write(f"meson setup failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}\n")
            return False

        # 2. Build via Ninja
        print(f"Step 2: {ninja_bin} -C {build_dir}")
        res = subprocess.run([ninja_bin, "-C", build_dir], capture_output=True, text=True)
        if res.returncode != 0:
            sys.stderr.write(f"ninja build failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}\n")
            return False

        # Verify Cargo target directory isolation: <build_dir>/cargo-target must exist
        cargo_target = os.path.join(build_dir, "cargo-target")
        if not os.path.isdir(cargo_target):
            sys.stderr.write(f"Error: Isolated cargo-target was not found at {cargo_target}\n")
            return False

        # 3. Install via Ninja
        print(f"Step 3: {ninja_bin} -C {build_dir} install")
        res = subprocess.run([ninja_bin, "-C", build_dir, "install"], capture_output=True, text=True)
        if res.returncode != 0:
            sys.stderr.write(f"ninja install failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}\n")
            return False

        # 4. Strict artifact inspection and permission verification
        print("Step 4: Strict inspection of installed artifacts and filesystem permissions...")
        daemon_bin = os.path.join(prefix_dir, "bin", "watchai-daemon")
        if not os.path.isfile(daemon_bin):
            sys.stderr.write(f"Error: Installed daemon binary missing at {daemon_bin}\n")
            return False
        if not os.access(daemon_bin, os.X_OK):
            sys.stderr.write("Error: Installed daemon binary is not executable\n")
            return False

        daemon_mode = stat.S_IMODE(os.stat(daemon_bin).st_mode)
        if daemon_mode != 0o755:
            sys.stderr.write(
                f"Error: Installed daemon mode is {oct(daemon_mode)}, strictly required to be 0o755\n"
            )
            return False

        # Assert watchai-mock and test infrastructure are strictly absent
        mock_bin = os.path.join(prefix_dir, "bin", "watchai-mock")
        if os.path.exists(mock_bin):
            sys.stderr.write("Error: watchai-mock found in install prefix! Must be strictly excluded.\n")
            return False

        # Check for any test sockets or harness artifacts
        for root, _, files in os.walk(prefix_dir):
            for fname in files:
                if "mock" in fname.lower() or "test" in fname.lower() and not fname.endswith(".service"):
                    sys.stderr.write(f"Error: Test artifact found in install prefix: {os.path.join(root, fname)}\n")
                    return False

        # Extension files verification
        ext_dir = os.path.join(
            prefix_dir,
            "share",
            "gnome-shell",
            "extensions",
            "watchai@gnome.org",
        )
        expected_ext_files = [
            "dbus_client.js",
            "extension.js",
            "indicator.js",
            "metadata.json",
            "notifications.js",
            "popover.js",
            "settings.js",
            "stylesheet.css",
            "utils.js",
        ]
        for fname in expected_ext_files:
            fpath = os.path.join(ext_dir, fname)
            if not os.path.isfile(fpath):
                sys.stderr.write(f"Error: Required extension file missing: {fpath}\n")
                return False
            fmode = stat.S_IMODE(os.stat(fpath).st_mode)
            if fmode & 0o022 != 0:
                sys.stderr.write(f"Error: Asset {fpath} is group/world writable: {oct(fmode)}\n")
                return False
            if fmode & 0o111 != 0:
                sys.stderr.write(f"Error: Asset {fpath} has unexpected executable bits set: {oct(fmode)}\n")
                return False

        # Metadata URL check
        with open(os.path.join(ext_dir, "metadata.json"), "r", encoding="utf-8") as f:
            ext_meta = json.load(f)
        if ext_meta.get("url") != "https://github.com/HA-ir/WatchAI":
            sys.stderr.write(f"Error: Installed extension has incorrect URL: {ext_meta.get('url')}\n")
            return False

        # Extension local schemas (dual-location strategy)
        local_schema_xml = os.path.join(
            ext_dir, "schemas", "org.gnome.shell.extensions.watchai.gschema.xml"
        )
        local_compiled = os.path.join(ext_dir, "schemas", "gschemas.compiled")
        if not os.path.isfile(local_schema_xml):
            sys.stderr.write(f"Error: Extension local schema XML missing: {local_schema_xml}\n")
            return False
        if not os.path.isfile(local_compiled):
            sys.stderr.write(f"Error: Extension local gschemas.compiled missing: {local_compiled}\n")
            return False

        for fpath in [local_schema_xml, local_compiled]:
            fmode = stat.S_IMODE(os.stat(fpath).st_mode)
            if fmode & 0o022 != 0 or fmode & 0o111 != 0:
                sys.stderr.write(f"Error: Local schema file {fpath} has unsafe mode: {oct(fmode)}\n")
                return False

        # System schemas: both XML and compiled database must exist
        sys_schema_dir = os.path.join(prefix_dir, "share", "glib-2.0", "schemas")
        sys_xml = os.path.join(
            sys_schema_dir,
            "org.gnome.shell.extensions.watchai.gschema.xml",
        )
        sys_compiled = os.path.join(sys_schema_dir, "gschemas.compiled")
        if not os.path.isfile(sys_xml):
            sys.stderr.write(f"Error: System schema XML missing: {sys_xml}\n")
            return False
        if not os.path.isfile(sys_compiled):
            sys.stderr.write(f"Error: System compiled schema missing: {sys_compiled}\n")
            return False

        for fpath in [sys_xml, sys_compiled]:
            fmode = stat.S_IMODE(os.stat(fpath).st_mode)
            if fmode & 0o022 != 0 or fmode & 0o111 != 0:
                sys.stderr.write(f"Error: System schema file {fpath} has unsafe mode: {oct(fmode)}\n")
                return False

        # Verify compiled schema validity using gsettings --schemadir
        gsettings_bin = shutil.which("gsettings")
        expected_keys = [
            "dwell-duration-seconds",
            "enable-desktop-notifications",
            "notify-on-waiting",
            "notify-on-error",
            "indicator-icon-style",
        ]
        if gsettings_bin:
            # 1. Test extension-local compiled schema
            ext_schema_dir = os.path.join(ext_dir, "schemas")
            res_ext = subprocess.run(
                [gsettings_bin, "--schemadir", ext_schema_dir, "list-keys", "org.gnome.shell.extensions.watchai"],
                capture_output=True,
                text=True,
            )
            if res_ext.returncode != 0:
                sys.stderr.write(
                    f"Error: Extension compiled schema query failed:\n{res_ext.stderr}\n"
                )
                return False
            ext_keys = set(res_ext.stdout.strip().splitlines())
            for req_key in expected_keys:
                if req_key not in ext_keys:
                    sys.stderr.write(
                        f"Error: Extension compiled schema missing required key '{req_key}'\nFound: {ext_keys}\n"
                    )
                    return False

            # 2. Test system-level compiled schema
            res_sys = subprocess.run(
                [gsettings_bin, "--schemadir", sys_schema_dir, "list-keys", "org.gnome.shell.extensions.watchai"],
                capture_output=True,
                text=True,
            )
            if res_sys.returncode != 0:
                sys.stderr.write(
                    f"Error: System compiled schema query failed:\n{res_sys.stderr}\n"
                )
                return False
            sys_keys = set(res_sys.stdout.strip().splitlines())
            for req_key in expected_keys:
                if req_key not in sys_keys:
                    sys.stderr.write(
                        f"Error: System compiled schema missing required key '{req_key}'\nFound: {sys_keys}\n"
                    )
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
            sys.stderr.write(f"Error: systemd service unit missing: {service_file}\n")
            return False

        service_mode = stat.S_IMODE(os.stat(service_file).st_mode)
        if service_mode & 0o022 != 0 or service_mode & 0o111 != 0:
            sys.stderr.write(f"Error: Service unit {service_file} has unsafe mode: {oct(service_mode)}\n")
            return False

        with open(service_file, "r", encoding="utf-8") as f:
            service_body = f.read()
        expected_exec = f"ExecStart={prefix_dir}/bin/watchai-daemon"
        if expected_exec not in service_body:
            sys.stderr.write(
                f"Error: Service unit missing expected ExecStart: {expected_exec}\nGot:\n{service_body}\n"
            )
            return False

        # 5. Comprehensive clean uninstallation
        print(f"Step 5: {ninja_bin} -C {build_dir} uninstall")
        res = subprocess.run([ninja_bin, "-C", build_dir, "uninstall"], capture_output=True, text=True)
        if res.returncode != 0:
            sys.stderr.write(f"ninja uninstall failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}\n")
            return False

        # Verify that all WatchAI artifacts are gone
        remaining_watchai_files = []
        remaining_compiled_schemas = []
        for root, _, files in os.walk(prefix_dir):
            for fname in files:
                fpath = os.path.join(root, fname)
                if "watchai" in fname.lower():
                    remaining_watchai_files.append(fpath)
                elif fname == "gschemas.compiled":
                    remaining_compiled_schemas.append(fpath)

        if remaining_watchai_files:
            sys.stderr.write(
                f"Error: Uninstallation incomplete! Residual WatchAI files remain:\n"
                + "\n".join(remaining_watchai_files)
                + "\n"
            )
            return False

        if remaining_compiled_schemas:
            sys.stderr.write(
                f"Error: Uninstallation incomplete! Residual gschemas.compiled files remain:\n"
                + "\n".join(remaining_compiled_schemas)
                + "\n"
            )
            return False

        if os.path.exists(daemon_bin):
            sys.stderr.write(f"Error: Daemon binary was not removed: {daemon_bin}\n")
            return False
        if os.path.exists(service_file):
            sys.stderr.write(f"Error: Service unit was not removed: {service_file}\n")
            return False
        if os.path.exists(ext_dir) and any(os.scandir(ext_dir)):
            sys.stderr.write(f"Error: Extension directory still contains files: {ext_dir}\n")
            return False
        if os.path.exists(sys_xml):
            sys.stderr.write(f"Error: System schema XML was not removed: {sys_xml}\n")
            return False
        if os.path.exists(sys_compiled):
            sys.stderr.write(f"Error: System compiled schema was not removed: {sys_compiled}\n")
            return False

        # 6. Reinstallation idempotency
        print(f"Step 6: {ninja_bin} -C {build_dir} install (reinstallation idempotency)")
        res = subprocess.run([ninja_bin, "-C", build_dir, "install"], capture_output=True, text=True)
        if res.returncode != 0:
            sys.stderr.write(f"ninja reinstall failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}\n")
            return False

        if not os.path.isfile(daemon_bin):
            sys.stderr.write(f"Error: Reinstall failed to materialize daemon binary: {daemon_bin}\n")
            return False
        if not os.path.isfile(local_compiled):
            sys.stderr.write(f"Error: Reinstall failed to materialize local compiled schema: {local_compiled}\n")
            return False
        if not os.path.isfile(sys_compiled):
            sys.stderr.write(f"Error: Reinstall failed to materialize system compiled schema: {sys_compiled}\n")
            return False
        if not os.path.isfile(service_file):
            sys.stderr.write(f"Error: Reinstall failed to materialize service unit: {service_file}\n")
            return False

        # Re-verify schema accessibility after reinstall
        if gsettings_bin:
            res_reinstall = subprocess.run(
                [gsettings_bin, "--schemadir", sys_schema_dir, "list-keys", "org.gnome.shell.extensions.watchai"],
                capture_output=True,
                text=True,
            )
            if res_reinstall.returncode != 0:
                sys.stderr.write(
                    f"Error: Reinstalled system schema query failed:\n{res_reinstall.stderr}\n"
                )
                return False
            reinstall_keys = set(res_reinstall.stdout.strip().splitlines())
            for req_key in expected_keys:
                if req_key not in reinstall_keys:
                    sys.stderr.write(
                        f"Error: Reinstalled system schema missing required key '{req_key}'\nFound: {reinstall_keys}\n"
                    )
                    return False

        print("\nAll dynamic packaging checks PASSED successfully!")
        return True


def main():
    parser = argparse.ArgumentParser(description="WatchAI packaging verification runner")
    parser.add_argument(
        "--static-only",
        action="store_true",
        help="Run static packaging asset checks only without requiring meson/ninja",
    )
    args = parser.parse_args()

    repo_root = os.path.abspath(
        os.path.join(os.path.dirname(__file__), "..", "..")
    )

    if args.static_only:
        success = check_static_packaging_assets(repo_root)
    else:
        success = run_full_packaging_verification(repo_root)

    if not success:
        sys.exit(1)

    print("\nPackaging verification completed successfully.")
    sys.exit(0)


if __name__ == "__main__":
    main()
