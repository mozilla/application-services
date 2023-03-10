#!/bin/sh

set -ex

rsync -av "$(realpath "$MOZ_FETCHES_DIR"/libs)/" "$(realpath "$1")/"
