#!/usr/bin/env bash

# Run tests in order to produce coverage data. Does not collect coverage data,
# just produces it. Note that we ignore all test failures -- some of the
# experimental flags we use cause test failures, we don't really care, other CI
# tasks check this anyway.
#
# Note that running this outside of CI doesn't work on all platforms. In
# particular, macos needs further patches in order to work, probably windows
# too. It's really intended for use inside CI only.
#
# Finally, this must be done on nightly -- `cargo-tarpaulin`, which works on
# stable, also has issues with NSS, but not ones that seem easy to work around.
# As a result, we use `grcov`. Ultimately `grcov` should long-term allow us to
# integrate coverage data from over the FFI, which is a huge plus to this
# approach, so it's probably for the best, even if `grcov`'s actual coverage
# information seems to be of lower quality than `tarpaulin`.

if [[ ! -f "$PWD/automation/emit_coverage_info.sh" ]]
then
    echo "emit_coverage_info.sh must be executed from the root directory."
    exit 1
fi

cargo +nightly clean
# Note that incremental builds, as well as sccache seems to break coverage :(.
# So, we disable those.
export CARGO_INCREMENTAL=0
unset RUSTC_WRAPPER
# TODO: re-add `-Clink-dead-code` to RUSTFLAGS -- right now it causes a build
# linker error in NSS, as apparently some part of NSS we don't use doesn't have
# the build flags set up correctly. This makes our code coverage accuracy worse,
# but AFAICT, not in a massive way -- or at least codecov.io seems to account
# for it in some manner.
#
# Also, this `--cfg coverage` is how the `#[cfg(coverage)]` we use in some parts
# of the rust code comes into existence (it's non-standard)
export RUSTFLAGS="-Zprofile -Ccodegen-units=1 -Copt-level=0 -Coverflow-checks=off -Zpanic_abort_tests -Cpanic=abort --cfg coverage";
export RUSTDOCFLAGS="-Cpanic=abort"

# Apparently --no-default-features doesn't work in the root, even with -p to select a specific package.
# Instead we pull the list of individual package manifest files which have default features
# out of `cargo metadata` and test using --manifest-path for each individual package.
for manifest in $(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select((.features | .default | length) > 0) | .manifest_path'); do
    package=$(dirname "$manifest")
    package=$(basename "$package")
    echo "## no-default-features test for package $package (manifest @ $manifest)"
    cargo +nightly test --manifest-path "$manifest" --no-default-features --no-fail-fast || true
done

cargo +nightly test --all --no-fail-fast || true

cargo +nightly run -p sync-test || true
cargo +nightly run -p protobuf-gen -- tools/protobuf_files.toml || true
cargo +nightly run -p systest || true

env RUSTFLAGS="--cfg __appsvc_ci_hack $RUSTFLAGS" cargo +nightly test --all --all-features --no-fail-fast || true
