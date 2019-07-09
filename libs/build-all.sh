#!/usr/bin/env bash

set -euvx

OPENSSL_VERSION="1.1.1a"
OPENSSL_SHA256="fc20130f8b7cbd2fb918b2f14e2f429e109c31ddd0fb38fc5d71d9ffed3f9f41"

# SQLCIPHER_VERSION="4.1.0"
# SQLCIPHER_SHA256="65144ca3ba4c0f9cd4bae8c20bb42f2b84424bf29d1ebcf04c44a728903b1faa"

NSS="nss-3.44"
# NSS_ARCHIVE="nss-3.44-with-nspr-4.21.tar.gz"
# NSS_URL="http://ftp.mozilla.org/pub/security/nss/releases/NSS_3_44_RTM/src/${NSS_ARCHIVE}"
# NSS_SHA256="298d86e18e96660d3c98476274b5857b48c135d809a10d6528d8661bdf834a49"

# End of configuration.

if [ ! -f "$(pwd)/build-all.sh" ]
then
    echo "build-all.sh must be executed from within the libs/ directory."
    exit 1
fi

if [ "${#}" -ne 1 ]
then
    echo "Usage:"
    echo "./build-all.sh [ios|android|desktop]"
    exit 1
fi

PLATFORM="${1}"

abspath () { case "${1}" in /*)printf "%s\\n" "${1}";; *)printf "%s\\n" "${PWD}/${1}";; esac; }
export -f abspath

OPENSSL="openssl-${OPENSSL_VERSION}"
rm -rf "${OPENSSL}"
if [ ! -e "${OPENSSL}.tar.gz" ]; then
  echo "Downloading ${OPENSSL}.tar.gz"
  curl -L -O "https://www.openssl.org/source/${OPENSSL}.tar.gz"
else
  echo "Using ${OPENSSL}.tar.gz"
fi
echo "${OPENSSL_SHA256}  ${OPENSSL}.tar.gz" | shasum -a 256 -c - || exit 2
tar xfz "${OPENSSL}.tar.gz"
OPENSSL_SRC_PATH=$(abspath ${OPENSSL})

# Delete the following...
rm -rf sqlcipher
git clone --single-branch --branch nss-crypto-impl --depth 1 "https://github.com/eoger/sqlcipher.git"
SQLCIPHER_SRC_PATH=$(abspath "sqlcipher")
# ... and uncomment the following once SQLCipher has an NSS crypto backend.
# SQLCIPHER="v${SQLCIPHER_VERSION}"
# rm -rf "${SQLCIPHER}"
# if [ ! -e "${SQLCIPHER}.tar.gz" ]; then
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
hg clone https://hg.mozilla.org/projects/nss/ -r 0c5d37301637ed024de8c2cbdbecf144aae12163 "${NSS}"/nss
# Temporary fix for bug 1561953
git clone --single-branch --branch without-versions https://github.com/eoger/nspr.git "${NSS}"/nspr
# hg clone https://hg.mozilla.org/projects/nspr/ -r cc73b6c7dab2e8053533e1f2c0c23dc721e10b76 "${NSS}"/nspr
# ... and uncomment the following once NSS 3.45 and NSPR 4.22 are out.
# if [ ! -e "${NSS_ARCHIVE}" ]; then
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

if [ "${PLATFORM}" == "ios" ]
then
  ./build-all-ios.sh "${OPENSSL_SRC_PATH}" "${SQLCIPHER_SRC_PATH}" "${NSS_SRC_PATH}"
elif [ "${PLATFORM}" == "android" ]
then
  ./build-all-android.sh "${OPENSSL_SRC_PATH}" "${SQLCIPHER_SRC_PATH}" "${NSS_SRC_PATH}"
elif [ "${PLATFORM}" == "desktop" ]
then
  ./build-nss-desktop.sh "${NSS_SRC_PATH}"
  ./build-openssl-desktop.sh "${OPENSSL_SRC_PATH}"
  ./build-sqlcipher-desktop.sh "${SQLCIPHER_SRC_PATH}"
elif [ "${PLATFORM}" == "darwin" ] || [ "${PLATFORM}" == "win32-x86-64" ]
then
  ./build-nss-desktop.sh "${NSS_SRC_PATH}" "${PLATFORM}"
  ./build-openssl-desktop.sh "${OPENSSL_SRC_PATH}" "${PLATFORM}"
  ./build-sqlcipher-desktop.sh "${SQLCIPHER_SRC_PATH}" "${PLATFORM}"
else
  echo "Unrecognized platform"
  exit 1
fi

echo "Cleaning up"
rm -rf "${OPENSSL_SRC_PATH}"
rm -rf "${SQLCIPHER_SRC_PATH}"
rm -rf "${NSS_SRC_PATH}"

echo "Done"
