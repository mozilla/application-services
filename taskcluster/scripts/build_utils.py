#!/usr/bin/env python3
"""Shared utilities for build scripts"""
import os
import platform
import subprocess
from pathlib import Path


def get_src_root():
    """Get repository root directory"""
    return Path(
        subprocess.check_output(["git", "rev-parse", "--show-toplevel"])
        .decode("utf8")
        .strip()
    ).resolve()


def setup_nss_environment(env, target, src_root):
    """
    Set up NSS_DIR environment for builds that use the app-svc feature.

    This should only be called for non-Windows, non-Android targets.

    Args:
        env: Environment dictionary to update
        target: Rust target triple (e.g., "x86_64-apple-darwin")
        src_root: Path to repository root

    Returns:
        Path to NSS directory
    """
    libs_dir = src_root / "libs"

    # Determine NSS directory candidates based on target
    nss_candidates = []
    if target.startswith("aarch64-apple-darwin"):
        nss_candidates = [
            libs_dir / "desktop" / "darwin-aarch64" / "nss",
            libs_dir / "desktop" / "darwin" / "nss",
        ]
    elif target.startswith("x86_64-apple-darwin"):
        nss_candidates = [
            libs_dir / "desktop" / "darwin-x86-64" / "nss",
            libs_dir / "desktop" / "darwin" / "nss",
        ]
    elif target.endswith("-linux-gnu"):
        nss_candidates = [
            libs_dir / "desktop" / "linux-x86-64" / "nss",
        ]
    else:
        raise ValueError(f"Unexpected target for NSS setup: {target}")

    # Find existing NSS or build it
    nss_dir = None
    for candidate in nss_candidates:
        if candidate.is_dir():
            nss_dir = candidate
            print(f"Using prebuilt NSS: {nss_dir}")
            break

    if not nss_dir:
        print("Building NSS locally...")
        platform_param = "desktop"
        subprocess.check_call(
            [str(libs_dir / "build-all.sh"), platform_param],
            env=env,
            cwd=str(libs_dir)
        )

        # Check again after build
        for candidate in nss_candidates:
            if candidate.is_dir():
                nss_dir = candidate
                break

        if not nss_dir:
            raise RuntimeError("NSS build completed but NSS_DIR not found")
        print(f"Built NSS: {nss_dir}")

    # Set environment variables for app-svc feature
    env["NSS_DIR"] = str(nss_dir)
    env["NSS_STATIC"] = "1"
    env["MOZ_AUTOMATION"] = "1"

    return nss_dir


def setup_libclang_path(env):
    """
    Help bindgen find libclang by setting LIBCLANG_PATH if not already set.

    Args:
        env: Environment dictionary to update
    """
    if "LIBCLANG_PATH" in env:
        return  # Already set

    host = platform.system().lower()
    if host == "darwin":
        libclang_paths = [
            "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib",
            "/Library/Developer/CommandLineTools/usr/lib",
        ]
    else:
        libclang_paths = [
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib/llvm-14/lib",
            "/usr/lib/llvm-15/lib",
            "/usr/lib/llvm-16/lib",
            "/usr/lib",
        ]

    for path in libclang_paths:
        if os.path.exists(path):
            env["LIBCLANG_PATH"] = path
            print(f"Using libclang: {path}")
            return


def needs_nss_setup(target):
    """
    Returns True if the target needs NSS setup.

    Windows and Android use rust-hpke (pure Rust), so they don't need NSS.
    Desktop platforms (macOS, Linux) use NSS via the app-svc feature.

    Args:
        target: Rust target triple

    Returns:
        bool: True if NSS is needed for this target
    """
    # Windows uses rust-hpke
    if target == "x86_64-pc-windows-gnu":
        return False

    # Android uses rust-hpke
    if "android" in target:
        return False

    # Desktop platforms use NSS
    return True


def setup_cross_compile_aarch64_linux(env):
    """Set up cross-compilation for aarch64-unknown-linux-gnu"""
    env["RUSTFLAGS"] = (
        env.get("RUSTFLAGS", "") + " -C linker=aarch64-linux-gnu-gcc"
    ).strip()

    # Help bindgen/clang find headers when cross-compiling
    env["BINDGEN_EXTRA_CLANG_ARGS"] = (
        "--target=aarch64-unknown-linux-gnu "
        "--sysroot=/usr/aarch64-linux-gnu "
        "-I/usr/aarch64-linux-gnu/include"
    )


def setup_cross_compile_windows(env):
    """Set up cross-compilation for x86_64-pc-windows-gnu"""
    env["RUSTFLAGS"] = (
        env.get("RUSTFLAGS", "") + " -C linker=x86_64-w64-mingw32-gcc"
    ).strip()
