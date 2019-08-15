#!/usr/bin/env bash

set -euvx

# SQLCIPHER_VERSION="4.1.0"
# SQLCIPHER_SHA256="65144ca3ba4c0f9cd4bae8c20bb42f2b84424bf29d1ebcf04c44a728903b1faa"

NSS="nss-3.44"
# NSS_ARCHIVE="nss-3.44-with-nspr-4.21.tar.gz"
# NSS_URL="http://ftp.mozilla.org/pub/security/nss/releases/NSS_3_44_RTM/src/${NSS_ARCHIVE}"
# NSS_SHA256="298d86e18e96660d3c98476274b5857b48c135d809a10d6528d8661bdf834a49"

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

# Delete the following...
rm -rf sqlcipher
git clone --single-branch --branch nss-crypto-impl --depth 1 "https://github.com/eoger/sqlcipher.git"
SQLCIPHER_SRC_PATH=$(abspath "sqlcipher")
# ... and uncomment the following once SQLCipher has an NSS crypto backend.
# SQLCIPHER="v${SQLCIPHER_VERSION}"
# rm -rf "${SQLCIPHER}"
# if [[ ! -e "${SQLCIPHER}.tar.gz" ]]; then
#   echo "Downloading ${SQLCIPHER}.tar.gz"
#   curl -L -O "https://github.com/sqlcipher/sqlcipher/archive/${SQLCIPHER}.tar.gz"
# else
#   echo "Using ${SQLCIPHER}.tar.gz"
# fi
# echo "${SQLCIPHER_SHA256}  ${SQLCIPHER}.tar.gz" | shasum -a 256 -c - || exit 2
# tar xfz "${SQLCIPHER}.tar.gz"
# SQLCIPHER_SRC_PATH=$(abspath "sqlcipher-${SQLCIPHER_VERSION}")

rm -rf "${NSS}"
# Delete the following...
hg clone https://hg.mozilla.org/projects/nss/ -r caf5d97f786fc3dad2f9318b228007a233b5f0bd "${NSS}"/nss
hg clone https://hg.mozilla.org/projects/nspr/ -r bf5c37749c28c523134e41b1e28b6ffaf2d2c4e9 "${NSS}"/nspr
# ... and uncomment the following once NSS 3.45 and NSPR 4.22 are out.
# if [[ ! -e "${NSS_ARCHIVE}" ]]; then
#   echo "Downloading ${NSS_ARCHIVE}"
#   curl -L -O "${NSS_URL}"
# else
#   echo "Using ${NSS_ARCHIVE}"
# fi
# echo "${NSS_SHA256}  ${NSS_ARCHIVE}" | shasum -a 256 -c - || exit 2
# tar xfz "${NSS_ARCHIVE}"
NSS_SRC_PATH=$(abspath "${NSS}")

# Some NSS symbols clash with OpenSSL symbols, rename them using
# C preprocessor define macros.
echo $'\
diff -r 65efa74ef84a coreconf/config.gypi
--- a/coreconf/config.gypi      Thu May 16 09:43:04 2019 +0000
+++ b/coreconf/config.gypi      Thu May 23 19:46:44 2019 -0400
@@ -138,6 +138,21 @@
       \'<(nspr_include_dir)\',
       \'<(nss_dist_dir)/private/<(module)\',
     ],
+    \'defines\': [
+      \'HMAC_Update=NSS_HMAC_Update\',
+      \'HMAC_Init=NSS_HMAC_Init\',
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
