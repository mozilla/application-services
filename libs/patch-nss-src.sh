#!/usr/bin/env bash

# This script patches the NSS/NSPR source code so we can cross-compile
# them properly until the fixes are merged upstream and released.

set -euvx

if [ "${#}" -ne 1 ]
then
    echo "Usage:"
    echo "./patch-nss-src.sh <NSS_SRC_PATH>"
    exit 1
fi

NSS_SRC_PATH=${1}

# Remove once NSS 3.44 is out (see bug 1540205 for context).
echo '\
--- chacha20poly1305.c	2019-03-15 20:25:08.000000000 -0400
+++ chacha20poly1305.c.patched	2019-03-29 17:24:37.000000000 -0400
@@ -157,6 +157,7 @@
 #endif
 }

+#ifndef NSS_DISABLE_CHACHAPOLY
 void
 ChaCha20Xor(uint8_t *output, uint8_t *block, uint32_t len, uint8_t *k,
             uint8_t *nonce, uint32_t ctr)
@@ -167,6 +168,7 @@
         Hacl_Chacha20_chacha20(output, block, len, k, nonce, ctr);
     }
 }
+#endif

 SECStatus
 ChaCha20Poly1305_Seal(const ChaCha20Poly1305Context *ctx, unsigned char *output,
' | patch ${NSS_SRC_PATH}/nss/lib/freebl/chacha20poly1305.c

# TODO: file bug to get this upstream.
echo '\
--- configure	2019-03-07 05:04:05.000000000 -0500
+++ configure.patched	2019-04-02 15:04:27.000000000 -0400
@@ -2641,6 +2641,12 @@
 
 
 case "$target" in
+x86_64-linux*-android*)
+    android_tool_prefix="x86_64-linux-android"
+    ;;
+aarch64-linux*-android*)
+    android_tool_prefix="aarch64-linux-android"
+    ;;
 arm-linux*-android*|*-linuxandroid*)
     android_tool_prefix="arm-linux-androideabi"
     ;;
' | patch ${NSS_SRC_PATH}/nspr/configure

# TODO: file bug to get this upstream.
echo '\
--- Linux.mk	2019-04-02 14:55:31.000000000 -0400
+++ Linux.mk.patched	2019-04-02 14:55:32.000000000 -0400
@@ -135,6 +135,10 @@
 endif
 OS_LIBS			= $(OS_PTHREAD) -ldl -lc

+ifeq ($(OS_TARGET),Android)
+	OS_LIBS		+= -llog
+endif
+
 ifdef USE_PTHREADS
 	DEFINES		+= -D_REENTRANT
 endif
' | patch ${NSS_SRC_PATH}/nss/coreconf/Linux.mk

# TODO: file bug to get this upstream.
# This param makes mingw32 gcc trip.
sed -i -e '/w44996/d' "${NSS_SRC_PATH}"/nss/lib/sqlite/Makefile

# We probably don't want to upstream this.
# Only build NSS lib and skip tests
sed -i -e '/^DIRS = /s/ cmd cpputil gtests$//' "${NSS_SRC_PATH}"/nss/manifest.mn
