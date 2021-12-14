#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Small script to be run on the CI server to cross compile nimbus-fml on Android
# and iOS developer machines.
#
# It installs the tools and targets needed via brew and rustup, cross compiles the nimbus-fml
# and then zips up all OS/architecture versions into a single archive.
#
# CircleCI then stores this as an artifact, and pushes it to Github on each release.
#
# This will be downloaded and unzipped as part of the buid processes for iOS and Android.
targets="aarch64-apple-darwin x86_64-unknown-linux-musl x86_64-apple-darwin"
dry_run=false

# Assume we're in $root_dir/components/support/nimbus-fml/scripts
root_dir=$(dirname "$0")/../../../..
fml_dir=$root_dir/components/support/nimbus-fml
target_dir=$root_dir/target
filename=$(basename "$fml_dir")

# But we'll dump the zip file wherever we run the script from.
dist_file=${filename}.zip

prompt='$'
# Compiling for Linux, getting the tools from homebrew.
# https://blog.filippo.io/easy-windows-and-linux-cross-compilers-for-macos/
# We'd like to run nimbus-fml on developer machines and the Android CIs (which are linux)
echo "## Installing tools for cross compiling"
install_musl_cross="brew install filosottile/musl-cross/musl-cross"
cargo_clean="cargo clean"
if [[ $dry_run != "true" ]] ; then
    $install_musl_cross
    $cargo_clean
else
    echo "$prompt $install_musl_cross"
    echo "$prompt $cargo_clean"
fi

# Start creating a zip command with the zipfile
zipfile="$(pwd)/$dist_file"
zip_cmd="zip $zipfile"

for TARGET in $targets ; do
    echo
    echo "## Cross compiling for $TARGET"
    rustup="rustup target add $TARGET"
    cargo_build="cargo build --release --target $TARGET"

    if [[ $dry_run != "true" ]] ; then
        $rustup
        (cd "$fml_dir" && $cargo_build)
    else
        echo "$prompt $rustup"
        echo "$prompt (cd $fml_dir && $cargo_build )"
    fi

    # Keep building the zip command with the commands as we build them.
    zip_cmd="$zip_cmd $TARGET/release/$filename"
done

# Finish up by executing the zip command.
echo
echo "## Preparing dist archive"
checksum="shasum -a 256 $zipfile"
if [[ $dry_run != "true" ]] ; then
    (cd "$target_dir" && $zip_cmd )
    $checksum > "$filename.sha256"
else
    echo "$prompt (cd $target_dir ; $zip_cmd )"
    echo "$prompt $checksum > $filename.sha256"
fi