#!/usr/bin/env bash

# This script cross-compiles NSS and NSPR for macOS from Linux.
# It is specifically designed for darwin cross-compilation in CI.

set -euvx

if [[ "${#}" -lt 1 ]] || [[ "${#}" -gt 2 ]]
then
  echo "Usage:"
  echo "./build-nss-macos-cross.sh <ABSOLUTE_SRC_DIR> [CROSS_COMPILE_TARGET]"
  exit 1
fi

NSS_SRC_DIR=${1}
CROSS_COMPILE_TARGET=${2:-darwin-aarch64}

# Set architecture-specific variables based on target
if [[ "${CROSS_COMPILE_TARGET}" == "darwin-aarch64" ]]; then
  DIST_DIR=$(abspath "desktop/darwin-aarch64/nss")
  NSS_TARGET="aarch64-apple-darwin"
  GYP_ARCH="arm64"
elif [[ "${CROSS_COMPILE_TARGET}" == "darwin-x86-64" ]]; then
  DIST_DIR=$(abspath "desktop/darwin-x86-64/nss")
  NSS_TARGET="x86_64-apple-darwin"
  GYP_ARCH="x64"
else
  echo "Unsupported cross-compile target: ${CROSS_COMPILE_TARGET}"
  exit 1
fi

if [[ -d "${DIST_DIR}" ]]; then
  echo "${DIST_DIR} folder already exists. Skipping build."
  exit 0
fi

# Read toolchain configuration from ORG_GRADLE_PROJECT environment variables
# These are set by cross-compile-setup.sh in CI
RUST_ANDROID_PREFIX=$(echo "ORG_GRADLE_PROJECT_RUST_ANDROID_GRADLE_TARGET_${NSS_TARGET}" | tr '[:lower:]-' '[:upper:]_')

# Check that NSS_DIR is set to detect CI environment
nss_dir_var="${RUST_ANDROID_PREFIX}_NSS_DIR"
if [[ -z "${!nss_dir_var}" ]]; then
  echo "Error: ${nss_dir_var} is not set"
  echo "This script must be run in a CI environment with cross-compile-setup.sh sourced"
  exit 1
fi

# Use toolchain configuration from environment
eval "BASE_CC=\$${RUST_ANDROID_PREFIX}_CC"
eval "AR=\$${RUST_ANDROID_PREFIX}_AR"
eval "AS=\$${RUST_ANDROID_PREFIX}_AS"
eval "RANLIB=\$${RUST_ANDROID_PREFIX}_RANLIB"
eval "LD=\$${RUST_ANDROID_PREFIX}_LD"
eval "STRIP=\$${RUST_ANDROID_PREFIX}_TOOLCHAIN_PREFIX/${NSS_TARGET}-strip"
eval "TOOLCHAIN_BIN=\$${RUST_ANDROID_PREFIX}_TOOLCHAIN_PREFIX"
eval "CFLAGS=\$${RUST_ANDROID_PREFIX}_CFLAGS_${NSS_TARGET//-/_}"

# Create a wrapper directory for fake tools and compiler wrappers
WRAPPER_DIR=$(mktemp -d)

# Create compiler wrapper scripts that filter out incompatible Apple-specific flags
# and add C++ standard library include paths for cross-compilation
cat > "${WRAPPER_DIR}/cc-wrapper" << 'EOF'
#!/usr/bin/env bash
# Filter out -fasm-blocks and -mpascal-strings which clang-20 doesn't support for cross-compilation
args=()
for arg in "$@"; do
  if [[ "$arg" != "-fasm-blocks" && "$arg" != "-mpascal-strings" ]]; then
    args+=("$arg")
  fi
done
# Add clang's C++ standard library include path
args+=("-I/builds/worker/clang/include/c++/v1")
# REAL_CC may contain spaces (e.g., "clang-20 -target ..."), so we need to use eval
eval exec "${REAL_CC}" '"${args[@]}"'
EOF
chmod +x "${WRAPPER_DIR}/cc-wrapper"

# Set CC and CXX to use the wrapper with all flags baked in
export REAL_CC="${BASE_CC} ${CFLAGS}"
CC="${WRAPPER_DIR}/cc-wrapper"
CXX="${WRAPPER_DIR}/cc-wrapper"
export CC CXX

# Create a fake xcodebuild script and tool wrappers to allow gyp to use the mac flavor
# This tricks gyp into thinking Xcode is installed so it generates macOS-style build rules
cat > "${WRAPPER_DIR}/xcodebuild" << 'EOF'
#!/usr/bin/env bash
# Fake xcodebuild that returns a version for gyp's mac flavor
# Xcode 12.2 corresponds to macOS SDK 11.0 (Big Sur) and adds Apple Silicon support
echo "Xcode 12.2"
echo "Build version 12B45b"
EOF
chmod +x "${WRAPPER_DIR}/xcodebuild"

# Create unprefixed symlinks to cctools binaries that gyp's mac flavor expects
# The mac flavor expects tools like 'otool', 'libtool', 'lipo' without the target prefix
ln -s "${TOOLCHAIN_BIN}/${NSS_TARGET}-otool" "${WRAPPER_DIR}/otool"
ln -s "${TOOLCHAIN_BIN}/${NSS_TARGET}-libtool" "${WRAPPER_DIR}/libtool"
ln -s "${TOOLCHAIN_BIN}/${NSS_TARGET}-lipo" "${WRAPPER_DIR}/lipo"
ln -s "${TOOLCHAIN_BIN}/${NSS_TARGET}-nm" "${WRAPPER_DIR}/nm"

export PATH="${WRAPPER_DIR}:${PATH}"

# Work around Python 3 bug in Ubuntu 22.04 gyp package
# Create a wrapper that monkey-patches GetStdoutQuiet to fix bytes/str handling
GYP_WRAPPER=$(mktemp)
cat > "${GYP_WRAPPER}" << 'EOFGYP'
#!/usr/bin/env python3
import sys
import gyp.xcode_emulation

# Monkey-patch GetStdoutQuiet to fix Python 3 bytes/str bug
original_GetStdoutQuiet = gyp.xcode_emulation.GetStdoutQuiet
def patched_GetStdoutQuiet(cmdlist):
    import subprocess
    job = subprocess.Popen(cmdlist, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    out = job.communicate()[0]
    if job.returncode != 0:
        return None
    return out.rstrip(b'\n').decode('utf-8')
gyp.xcode_emulation.GetStdoutQuiet = patched_GetStdoutQuiet

# Now run gyp normally
import gyp
sys.exit(gyp.script_main())
EOFGYP
chmod +x "${GYP_WRAPPER}"

# Build NSPR
NSPR_BUILD_DIR=$(mktemp -d)
pushd "${NSPR_BUILD_DIR}"
"${NSS_SRC_DIR}"/nspr/configure \
  STRIP="${STRIP}" \
  RANLIB="${RANLIB}" \
  AR="${AR}" \
  AS="${AS}" \
  LD="${LD}" \
  CC="${CC}" \
  CCC="${CC}" \
  --target="${NSS_TARGET}" \
  --enable-64bit \
  --disable-debug \
  --enable-optimize
make
popd

# Build NSS using gyp
NSS_BUILD_DIR=$(mktemp -d)
rm -rf "${NSS_SRC_DIR}/nss/out"

"${GYP_WRAPPER}" -f ninja-mac "${NSS_SRC_DIR}/nss/nss.gyp" \
  --depth "${NSS_SRC_DIR}/nss/" \
  --generator-output=. \
  -DOS=mac \
  -Dnspr_lib_dir="${NSPR_BUILD_DIR}/dist/lib" \
  -Dnspr_include_dir="${NSPR_BUILD_DIR}/dist/include/nspr" \
  -Dnss_dist_dir="${NSS_BUILD_DIR}" \
  -Dnss_dist_obj_dir="${NSS_BUILD_DIR}" \
  -Dhost_arch="${GYP_ARCH}" \
  -Dtarget_arch="${GYP_ARCH}" \
  -Dstatic_libs=1 \
  -Ddisable_dbm=1 \
  -Dsign_libs=0 \
  -Denable_sslkeylogfile=0 \
  -Ddisable_tests=1 \
  -Ddisable_libpkix=1 \
  -Dpython=python3

GENERATED_DIR="${NSS_SRC_DIR}/nss/out/Release/"
echo "=== Dumping build.ninja for nss-macos-cross ==="
cat "${GENERATED_DIR}/build.ninja"

ninja -C "${GENERATED_DIR}" nss_static_libs freebl_static pk11wrap_static softokn_static
if [[ "${ARCH}" == "x86_64" ]]; then
  ninja -C "${GENERATED_DIR}" hw-acc-crypto-avx hw-acc-crypto-avx2 gcm-aes-x86_c_lib sha-x86_c_lib
fi
if [[ "${ARCH}" == "aarch64" ]] || [[ "${ARCH}" == "arm64" ]]; then
  ninja -C "${GENERATED_DIR}" gcm-aes-aarch64_c_lib armv8_c_lib
fi

# Assemble the DIST_DIR with relevant libraries and headers
./copy-nss-libs.sh \
  "mac" \
  "${GYP_ARCH}" \
  "${DIST_DIR}" \
  "${NSS_BUILD_DIR}/lib" \
  "${NSPR_BUILD_DIR}/dist/lib" \
  "${NSS_BUILD_DIR}/public/nss" \
  "${NSPR_BUILD_DIR}/dist/include/nspr"

# Cleanup
rm -rf "${NSS_BUILD_DIR}"
rm -rf "${NSPR_BUILD_DIR}"
rm -rf "${WRAPPER_DIR}"
