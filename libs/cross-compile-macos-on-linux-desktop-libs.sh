#!/usr/bin/env bash

# Install clang, a port of cctools, and the macOS SDK into /tmp.  This
# is all cribbed from mozilla-central; start at
# https://searchfox.org/mozilla-central/rev/39cb1e96cf97713c444c5a0404d4f84627aee85d/build/macosx/cross-mozconfig.common.

set -euvx

pushd /tmp

curl --location --retry 10 --retry-delay 10 -o tooltool.py https://raw.githubusercontent.com/mozilla/build-tooltool/1c6943c019fd378a337423a85c26932084edf771/tooltool.py
chmod +x tooltool.py

curl --location --retry 10 --retry-delay 10 -o cross-clang.manifest https://hg.mozilla.org/mozilla-central/raw-file/f7a97b344fa59bd3b01ea81ebd5b150aa63bfb12/browser/config/tooltool-manifests/macosx64/cross-clang.manifest

python tooltool.py --manifest=cross-clang.manifest --url=http://relengapi/tooltool/ fetch

# curl --location --retry 10 --retry-delay 10 -o cctools.tar.xz https://index.taskcluster.net/v1/task/gecko.cache.level-3.toolchains.v2.linux64-cctools-port.latest/artifacts/public/build/cctools.tar.xz
curl --location --retry 10 --retry-delay 10 -o cctools.tar.xz https://queue.taskcluster.net/v1/task/T-2QILzUSN-fEkRUH9bYvg/artifacts/public%2Fbuild%2Fcctools.tar.xz
tar xf cctools.tar.xz

# curl --location --retry 10 --retry-delay 10 -o clang.tar.xz https://index.taskcluster.net/v1/task/gecko.cache.level-3.toolchains.v2.linux64-clang-7.latest/artifacts/public/build/clang.tar.xz
curl --location --retry 10 --retry-delay 10 -o clang.tar.xz https://queue.taskcluster.net/v1/task/a2942WbJRgObZFIDwRc_OQ/artifacts/public%2Fbuild%2Fclang.tar.xz
tar xf clang.tar.xz

popd
