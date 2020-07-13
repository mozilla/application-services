#!/usr/bin/env bash
#
# A convenience wrapper for running the full suite of rust tests.
# This is complicated by rust's feature system, so we want to run:
#
#  1. Tests with only the default features on
#  2. Tests with all features on
#  3. Tests with no features on
#
# This is not perfect (really we want the cartesian product), but is good enough in practice.

set -eux

if [[ ! -f "$PWD/automation/all_rust_tests.sh" ]]
then
    echo "all_rust_tests.sh must be executed from the root directory."
    exit 1
fi

EXTRA_ARGS=( "$@" )

cargo test --all ${EXTRA_ARGS[@]:+"${EXTRA_ARGS[@]}"}

# Apparently --no-default-features doesn't work in the root, even with -p to select a specific package.
# Instead we pull the list of individual package manifest files which have default features
# out of `cargo metadata` and test using --manifest-path for each individual package.
for manifest in $(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select((.features | .default | length) > 0) | .manifest_path'); do
    package=$(dirname "$manifest")
    package=$(basename "$package")
    echo "## no-default-features test for package $package (manifest @ $manifest)"
    cargo test --manifest-path "$manifest" --no-default-features ${EXTRA_ARGS[@]:+"${EXTRA_ARGS[@]}"}
done

# Test NSS bindings
cargo run -p systest

# Test all features. nss-sys's --features handling is broken, and I don't feel
# like shaving this yak any further today so just use a custom --cfg. This
# shouldn't be this way!
#
# Do this last to minimize rebuild churn.
env RUSTFLAGS="--cfg __appsvc_ci_hack" cargo test --all --all-features ${EXTRA_ARGS[@]:+"${EXTRA_ARGS[@]}"}
