#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

set -e

# Small script to be run on the CI server to cross compile a command line project on Android
# and iOS developer machines.
#
# It installs the tools and targets needed via brew and rustup, cross compiles the project
# and then zips up all OS/architecture versions into a single archive.
#
# CircleCI then stores this as an artifact, and pushes it to Github on each release.
#
# This will be downloaded and unzipped as part of the build processes for iOS and Android.
TARGETS="aarch64-apple-darwin x86_64-unknown-linux-musl x86_64-apple-darwin x86_64-pc-windows-gnu"
DRY_RUN=false
DIRTY=false
PROJECT_DIR=
DIST_DIR=$(pwd)
INSTALL_TOOLS_ONLY=false

# Assume we're in $root_dir/build-scripts
root_dir=$(dirname "$0")/..
target_dir=$root_dir/target

while (( "$#" )); do
    case "$1" in
        -p|--project)
            PROJECT_DIR="$2"
            shift 2
            ;;
        -d|--dist-dir)
            DIST_DIR="$2"
            shift 2
            ;;
        --dirty)
            DIRTY="true"
            shift
            ;;
        --targets)
            TARGETS="$2"
            shift 2
            ;;
        --install-only)
            INSTALL_TOOLS_ONLY="true"
            shift
            ;;
        --dry-run)
            DRY_RUN="true"
            shift
            ;;
        *)
            echo "'$1' not supported"  1>&2
            exit 1
            ;;
    esac
done

function maybeRun {
    local prompt='$'
    local cmd=$1
    if [[ $DRY_RUN != "true" ]] ; then
        echo "$prompt $cmd"
        eval "$cmd"
    else
        echo "$prompt $cmd"
    fi
}

echo "## Installing tools for cross compiling"

for target in $TARGETS ; do
    echo
    echo "# Installing tools for $target"

    case "$target" in
        x86_64-pc-windows-gnu)
            maybeRun "brew install mingw-w64"
            ;;
        x86_64-unknown-linux-gnu)
            maybeRun "brew install messense/macos-cross-toolchains/x86_64-unknown-linux-gnu"
            ;;
        x86_64-unknown-linux-musl)
            # Compiling for Linux, getting the tools from homebrew.
            # https://blog.filippo.io/easy-windows-and-linux-cross-compilers-for-macos/
            # We'd like to run the binary on developer machines and the Android CIs (which are linux)
            maybeRun "brew install filosottile/musl-cross/musl-cross"
            ;;
        *)
            ;;
    esac
done

if [[ $INSTALL_TOOLS_ONLY == "true" ]] ; then
    exit 0
fi

if [ -z "$PROJECT_DIR" ] ; then
    echo "Require a project directory" 1>&2
    exit 1
fi

basename=$(basename "$PROJECT_DIR")

# But we'll dump the zip file wherever we run the script from.
dist_file=${basename}.zip

if [[ $DIRTY != "true" ]] ; then
    maybeRun "cargo clean --manifest-path $PROJECT_DIR/Cargo.toml"
fi

# Start creating a zip command with the zipfile
files_to_zip=""

for target in $TARGETS ; do
    echo
    echo "## Cross compiling for $target"

    maybeRun "rustup target add $target"

    cargo_target="$target"
    binary_name="$basename"

    case "$target" in
        x86_64-pc-windows-gnu)
            cargo_target=x86_64-pc-windows-gnu
            binary_name="$basename.exe"
            ;;
        x86_64-unknown-linux-gnu)
            # Fixes the 'ld: unknown option: --as-needed' error.
            # https://stackoverflow.com/a/68121888
            export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-unknown-linux-gnu-gcc
            ;;
        x86_64-unknown-linux-musl)
            # https://betterprogramming.pub/cross-compiling-rust-from-mac-to-linux-7fad5a454ab1
            export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-linux-musl-gcc
            # Instead of soft linking (ln -sf) from musl-cross, we set TARGET_CC here.
            export TARGET_CC=x86_64-linux-musl-gcc
            ;;
        *)
            ;;
    esac

    # Build everything!
    maybeRun "cargo build --manifest-path $PROJECT_DIR/Cargo.toml --release --target $cargo_target"

    # Keep building the zip command with the commands as we build them.
    files_to_zip="$files_to_zip $target/release/$binary_name"

    # Now unset the environment variables we just used, so it doesn't upset the next
    # way around the loop.
    case "$target" in
        x86_64-unknown-linux-gnu)
            unset CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER
            ;;
        x86_64-unknown-linux-musl)
            unset CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER
            unset TARGET_CC
            ;;
        *)
            ;;
    esac
done

# Finish up by executing the zip command.
echo
echo "## Preparing dist archive"
maybeRun "pushd $target_dir"
maybeRun "zip $DIST_DIR/$dist_file $files_to_zip"
maybeRun "popd"
maybeRun "pushd $DIST_DIR"
maybeRun "shasum -a 256 $dist_file > $basename.sha256"
maybeRun "popd"
