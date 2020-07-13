#!/usr/bin/env bash
#
# A convenience wrapper for running clippy across all packages.
# This is complicated by rust's feature system, so we want to run:
#
#  1. Clippy with all features enabled
#  2. Clippy with no features enabled
#

set -eux

if [[ ! -f "$PWD/automation/all_clippy_checks.sh" ]]
then
    echo "all_clippy_checks.sh must be executed from the root directory."
    exit 1
fi

EXTRA_ARGS=( "$@" )

cargo clippy --all --all-targets --all-features -- -D warnings ${EXTRA_ARGS[@]:+"${EXTRA_ARGS[@]}"}

# Apparently --no-default-features doesn't work in the root, even with -p to select a specific package.
# Instead we pull the list of individual package manifest files which have default features
# out of `cargo metadata` and test using --manifest-path for each individual package.
for manifest in $(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select((.features | .default | length) > 0) | .manifest_path'); do
    package=$(dirname "$manifest")
    package=$(basename "$package")
    echo "## no-default-features clippy for package $package (manifest @ $manifest)"
    cargo clippy --manifest-path "$manifest" --all-targets --no-default-features -- -D warnings ${EXTRA_ARGS[@]:+"${EXTRA_ARGS[@]}"}
done
