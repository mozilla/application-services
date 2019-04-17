# Set environment variables for using vendored dependencies in desktop builds.
#
# This file should be used via `source ./libs/bootstrap-desktop.sh` and will
# not have the desired effect if you try to run it directly, because it
# uses `export` to set environment variables.

if [ ! -f "$(pwd)/libs/build-all.sh" ]; then
  echo "ERROR: bootstrap-desktop.sh should be run from the root directory of the repo"
else
  if [ $(uname -s) == "Darwin" ]; then
    APPSERVICES_PLATFORM_DIR="$(pwd)/libs/desktop/darwin"
  else
    APPSERVICES_PLATFORM_DIR="$(pwd)/libs/desktop/linux-x86-64"
  fi
  export SQLCIPHER_LIB_DIR="${APPSERVICES_PLATFORM_DIR}/sqlcipher/lib"
  export SQLCIPHER_INCLUDE_DIR="${APPSERVICES_PLATFORM_DIR}/sqlcipher/include"
  export OPENSSL_DIR="${APPSERVICES_PLATFORM_DIR}/openssl"
  export NSS_DIR="${APPSERVICES_PLATFORM_DIR}/nss"
  if [ ! -d "${SQLCIPHER_LIB_DIR}" -o ! -d "${OPENSSL_DIR}" -o ! -d "${NSS_DIR}" ]; then
    pushd libs && ./build-all.sh desktop && popd
  fi;

  # NSS system libs check.
  pushd "$(mktemp -d)"
  echo '
  use std::{ffi::*, os::raw::*, process::exit};
  extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
  }

  #[cfg(target_os = "macos")]
  const LIB_NAME: &str = "libnss3.dylib";
  #[cfg(target_os = "linux")]
  const LIB_NAME: &str = "libnss3.so";
  const RTLD_LAZY: c_int = 0x01;
  fn main() {
    let result = unsafe { dlopen(CString::new(LIB_NAME).unwrap().as_ptr(), RTLD_LAZY) };
    if result.is_null() { println!("Could not dlopen nss"); exit(1); }
  }
  ' > nsslibcheck.rs
  rustc nsslibcheck.rs && ./nsslibcheck
  LIB_CHECK_RES="${?}"
  popd
  if [[ "${LIB_CHECK_RES}" != 0 ]]; then
    echo "It looks like the NSS libraries are not installed system-wide on your computer. Tests might fail to run."
    echo "* On MacOS:"
    echo "brew install nss"
    echo "brew link --force nss"
    echo "* On Debian/Ubuntu:"
    echo "apt-get install libnss3-dev"
  fi
fi
