#!/usr/bin/env python3

from pathlib import Path
import shutil
import sys
import subprocess

crate_dir = Path(__file__).parent
root_dir = crate_dir.parent.parent
fairy_bridge_crate_dir = root_dir / 'components' / 'fairy-bridge'
target_dir = root_dir / 'target'
target_debug = target_dir / 'debug'
work_dir = target_dir / 'fairy-bridge-cli'

# build everything
def find_dylib():
    for prefix in ["lib", ""]:
        for ext in ["so", "DLL", "dylib"]:
            lib_path = target_debug / f"{prefix}fairy_bridge_cli.{ext}"
            if lib_path.exists():
                return lib_path
    raise LookupError("Can't find dylib")
if work_dir.exists():
    shutil.rmtree(work_dir)
work_dir.mkdir(parents=True)
subprocess.check_call(["cargo", "build"], cwd=crate_dir)
shutil.copy(crate_dir / "src" / "__main__.py", work_dir)
(work_dir / "__init__.py").touch()
dylib_path = find_dylib()
# TODO: make this less unix specific
subprocess.check_call([
    "g++", "--shared", "-fPIC",
    "-lcurl",
    "-I", fairy_bridge_crate_dir / "c-backend-include",
    crate_dir / "src" / "fairy_bridge_backend.cpp",
    dylib_path,
    "-o", work_dir / "libfairy_bridge_cli.so"
])
subprocess.check_call(
    [
        "cargo", "run", "-p", "embedded-uniffi-bindgen", "--", "generate", "-l", "python",
        "--library", dylib_path.absolute(),
        "--out-dir", work_dir.absolute(),
    ],
    cwd=root_dir,
)

# run it
print()
print()
subprocess.check_call(
    ["/usr/bin/env", "python3", "-m", "fairy-bridge-cli"] + sys.argv[1:],
    cwd = target_dir
)

