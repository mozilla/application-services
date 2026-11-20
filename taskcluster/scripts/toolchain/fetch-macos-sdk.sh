#!/bin/bash
set -euxo pipefail

curl -sfSL --retry 5 --retry-delay 10 \
    http://taskcluster/api/index/v1/task/gecko.cache.level-3.toolchains.v3.macosx64-sdk-27.2.latest/artifacts/project%2Fgecko%2Fmac-sdk%2FMacOSX27.2.sdk.tar.zst \
    | tar -I zstd -x -C /builds/worker
