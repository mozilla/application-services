#!/usr/bin/env bash

set -euvx

if test -f bin/patchelf; then
    echo "Already have patchelf."
    return
fi

rm -f patchelf-master.zip bin/patchelf
rm -rf patchelf-master

mkdir -p bin

PATCHELF_COMMIT_HASH=27ffe8ae871e7a186018d66020ef3f6162c12c69
PATCHELF_ZIP_SHA256=efc301d64a34892f813b546754ddf63cc210b82292f34f74fde96cb29c1f76ee

curl -o patchelf-src.zip -L "https://github.com/NixOS/patchelf/archive/$PATCHELF_COMMIT_HASH.zip"

echo "${PATCHELF_ZIP_SHA256}  patchelf-src.zip" | shasum -a 256 -c - || exit 2
unzip patchelf-src.zip
# This is what's inside the zip
PATCHELF_SRC_DIR="patchelf-$PATCHELF_COMMIT_HASH"

cd $PATCHELF_SRC_DIR
mkdir .install_prefix
./bootstrap.sh
./configure --prefix="$PWD/.install_prefix"
# `patchelf` is just 1 C++ file so no need for -j6
make && make install

mv .install_prefix/bin/patchelf ../bin

cd -

rm -rf patchelf-src.zip $PATCHELF_SRC_DIR
