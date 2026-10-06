#!/usr/bin/env python3
"""Build helper script for compiling Cargo binaries within Meson."""

import argparse
import os
import shutil
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description="Compile Cargo package for Meson")
    parser.add_argument("--source-dir", required=True, help="Path to repository root")
    parser.add_argument("--build-dir", required=True, help="Path to Meson build directory")
    parser.add_argument("--output", required=True, help="Path to output binary destination")
    parser.add_argument("--package", default="watchai-daemon", help="Cargo package name to build")
    parser.add_argument(
        "--release",
        action="store_true",
        default=True,
        help="Build in release mode (default: True)",
    )

    args = parser.parse_args()

    manifest_path = os.path.abspath(os.path.join(args.source_dir, "Cargo.toml"))
    if not os.path.isfile(manifest_path):
        sys.stderr.write(f"Error: Manifest not found at {manifest_path}\n")
        return 1

    target_dir = os.path.abspath(os.path.join(args.build_dir, "cargo-target"))

    cmd = [
        "cargo",
        "build",
        "--manifest-path",
        manifest_path,
        "-p",
        args.package,
        "--target-dir",
        target_dir,
    ]
    if args.release:
        cmd.append("--release")

    print(f"Building {args.package} via Cargo: {' '.join(cmd)}")
    result = subprocess.run(cmd)
    if result.returncode != 0:
        sys.stderr.write(f"Error: Cargo build failed with exit code {result.returncode}\n")
        return result.returncode

    profile_dir = "release" if args.release else "debug"
    built_binary = os.path.join(target_dir, profile_dir, args.package)
    if not os.path.isfile(built_binary):
        sys.stderr.write(f"Error: Expected binary not found at {built_binary}\n")
        return 1

    output_dir = os.path.dirname(os.path.abspath(args.output))
    os.makedirs(output_dir, exist_ok=True)

    print(f"Copying {built_binary} -> {args.output}")
    shutil.copy2(built_binary, args.output)
    os.chmod(args.output, 0o755)

    return 0


if __name__ == "__main__":
    sys.exit(main())
