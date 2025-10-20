#!/bin/sh

set -ex

# Only copy the specific subdirectories we need
for dir in android desktop ios; do
    if [ -d "${MOZ_FETCHES_DIR}/libs/${dir}" ]; then
        mkdir -p "$(realpath "$1")/${dir}"
        rsync -a "${MOZ_FETCHES_DIR}/libs/${dir}/" "$(realpath "$1")/${dir}/"
    fi
done
