#!/usr/bin/env bash

set -euvx

SQLCIPHER_VERSION="4.4.0"
SQLCIPHER_SHA256="0924b2ae1079717954498bda78a30de20ce2a6083076b16214a711567821d148"

NSS="nss-3.56"
NSS_ARCHIVE="nss-3.56-with-nspr-4.28.tar.gz"
NSS_URL="https://ftp.mozilla.org/pub/security/nss/releases/NSS_3_56_RTM/src/${NSS_ARCHIVE}"
NSS_SHA256="989b548aa5589d15e31a306218d3c48dbc472b6043b78c6846b5acc54ebfed67"

# End of configuration.

if [[ ! -f "$(pwd)/build-all.sh" ]]
then
    echo "build-all.sh must be executed from within the libs/ directory."
    exit 1
fi

if [[ "${#}" -ne 1 ]]
then
    echo "Usage:"
    echo "./build-all.sh [ios|android|desktop]"
    exit 1
fi

PLATFORM="${1}"

abspath () { case "${1}" in /*)printf "%s\\n" "${1}";; *)printf "%s\\n" "${PWD}/${1}";; esac; }
export -f abspath

if ! [[ -x "$(command -v gyp)" ]]; then
  echo 'Error: gyp needs to be installed and executable. See https://github.com/mogemimi/pomdog/wiki/How-to-Install-GYP for install instructions.' >&2
  exit 1
fi

if ! [[ -x "$(command -v ninja)" ]]; then
  echo 'Error: ninja needs to be installed and executable. See https://github.com/ninja-build/ninja/wiki/Pre-built-Ninja-packages for install instructions.' >&2
  exit 1
fi

# SQLCipher needs TCL.
if ! [[ -x "$(command -v tclsh)" ]]; then
  echo 'Error: tclsh needs to be installed and executable. See https://www.tcl.tk/software/tcltk/.' >&2
  exit 1
fi

SQLCIPHER="v${SQLCIPHER_VERSION}"
rm -rf "${SQLCIPHER}"
if [[ ! -e "${SQLCIPHER}.tar.gz" ]]; then
 echo "Downloading ${SQLCIPHER}.tar.gz"
 curl -sfSL --retry 5 --retry-delay 10 -O "https://github.com/sqlcipher/sqlcipher/archive/${SQLCIPHER}.tar.gz"
else
 echo "Using ${SQLCIPHER}.tar.gz"
fi
echo "${SQLCIPHER_SHA256}  ${SQLCIPHER}.tar.gz" | shasum -a 256 -c - || exit 2
tar xfz "${SQLCIPHER}.tar.gz"
SQLCIPHER_SRC_PATH=$(abspath "sqlcipher-${SQLCIPHER_VERSION}")

rm -rf "${NSS}"
if [[ ! -e "${NSS_ARCHIVE}" ]]; then
  echo "Downloading ${NSS_ARCHIVE}"
  curl -sfSL --retry 5 --retry-delay 10 -O "${NSS_URL}"
else
  echo "Using ${NSS_ARCHIVE}"
fi
echo "${NSS_SHA256}  ${NSS_ARCHIVE}" | shasum -a 256 -c - || exit 2
tar xfz "${NSS_ARCHIVE}"
NSS_SRC_PATH=$(abspath "${NSS}")

# Some NSS symbols clash with OpenSSL symbols, rename them using
# C preprocessor define macros.
echo $'\
diff -r 65efa74ef84a coreconf/config.gypi
--- a/coreconf/config.gypi      Thu May 16 09:43:04 2019 +0000
+++ b/coreconf/config.gypi      Thu May 23 19:46:44 2019 -0400
@@ -138,6 +138,23 @@
       \'<(nspr_include_dir)\',
       \'<(nss_dist_dir)/private/<(module)\',
     ],
+    \'defines\': [
+      \'HMAC_Update=NSS_HMAC_Update\',
+      \'HMAC_Init=NSS_HMAC_Init\',
+      \'CMAC_Update=NSS_CMAC_Update\',
+      \'CMAC_Init=NSS_CMAC_Init\',
+      \'MD5_Update=NSS_MD5_Update\',
+      \'SHA1_Update=NSS_SHA1_Update\',
+      \'SHA256_Update=NSS_SHA256_Update\',
+      \'SHA224_Update=NSS_SHA224_Update\',
+      \'SHA512_Update=NSS_SHA512_Update\',
+      \'SHA384_Update=NSS_SHA384_Update\',
+      \'SEED_set_key=NSS_SEED_set_key\',
+      \'SEED_encrypt=NSS_SEED_encrypt\',
+      \'SEED_decrypt=NSS_SEED_decrypt\',
+      \'SEED_ecb_encrypt=NSS_SEED_ecb_encrypt\',
+      \'SEED_cbc_encrypt=NSS_SEED_cbc_encrypt\',
+    ],
     \'conditions\': [
       [ \'mozpkix_only==1 and OS=="linux"\', {
         \'include_dirs\': [
' | patch "${NSS_SRC_PATH}/nss/coreconf/config.gypi"

# Early return hack to prevent NSPR Android setup
# which does not work with ndk unified headers and clang.
echo $'\
@@ -2662,6 +2662,9 @@

 case "$target" in
 *-android*|*-linuxandroid*)
+    $as_echo "#define ANDROID 1" >>confdefs.h
+    ;;
+    unreachable)
     if test -z "$android_ndk" ; then
        as_fn_error $? "You must specify --with-android-ndk=/path/to/ndk when targeting Android." "$LINENO" 5
     fi
' | patch "${NSS_SRC_PATH}/nspr/configure"

if [[ "${PLATFORM}" == "ios" ]]
then
  ./build-all-ios.sh "${SQLCIPHER_SRC_PATH}" "${NSS_SRC_PATH}"
elif [[ "${PLATFORM}" == "android" ]]
then
  ./build-all-android.sh "${SQLCIPHER_SRC_PATH}" "${NSS_SRC_PATH}"
elif [[ "${PLATFORM}" == "desktop" ]]
then
  ./build-nss-desktop.sh "${NSS_SRC_PATH}"
  ./build-sqlcipher-desktop.sh "${SQLCIPHER_SRC_PATH}"
elif [[ "${PLATFORM}" == "darwin" ]] || [[ "${PLATFORM}" == "win32-x86-64" ]]
then
  ./build-nss-desktop.sh "${NSS_SRC_PATH}" "${PLATFORM}"
  ./build-sqlcipher-desktop.sh "${SQLCIPHER_SRC_PATH}" "${PLATFORM}"
else
  echo "Unrecognized platform"
  exit 1
fi

echo "Cleaning up"
rm -rf "${SQLCIPHER_SRC_PATH}"
rm -rf "${NSS_SRC_PATH}"

echo "Done"
