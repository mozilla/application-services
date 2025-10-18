#!/usr/bin/env python3
"""
Utilities for building application-services components.

This module provides helper functions for setting up build environments,
particularly for handling NSS dependencies.
"""

import os
import subprocess
from pathlib import Path


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
    elif target.endswith("-linux-gnu") or target.endswith("-linux-musl"):
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

    # Set LIBCLANG_PATH for bindgen
    if "LIBCLANG_PATH" not in env:
        env["LIBCLANG_PATH"] = "/usr/lib/x86_64-linux-gnu"

    return nss_dir


def needs_nss_setup(target):
    """
    Returns True if the target needs NSS setup.

    Only x86_64 desktop Linux/macOS use NSS. Everything else uses rust-hpke.
    """
    # Only x86_64 desktop platforms use NSS
    if target in ["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"]:
        return True
    if target in ["x86_64-apple-darwin"]:
        return True

    # Everything else (aarch64-linux, iOS, Android, Windows, etc.) uses rust-hpke
    return False
