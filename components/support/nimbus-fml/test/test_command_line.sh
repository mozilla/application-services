#!/usr/bin/env bash

# Find out where the FML root directory is.
this_file="${BASH_SOURCE[0]}"
parent="$(dirname $this_file)/.."

function fail {
    echo "Failure with: $*"
    exit 1
}

fml_root=$(cd "$parent" || fail "Can't find where the nimbus-fml directory is" ; pwd)
build_dir="$fml_root/build/cli_test"
fixtures="$fml_root/fixtures"

fml_file="$fixtures/fe/importing/simple/app.yaml"

if [ ! -f "$fml_file" ] ;
then
	fail "fml_root is not set properly: $fml_root"
fi

pushd "$fml_root" > /dev/null || fail "Can't change to FML root directory"

rm -Rf "$build_dir" 2>/dev/null
mkdir -p "$build_dir"

if ! cargo build \
	2> /dev/null ;
then
	fail "cargo build"
fi

# Legacy command line interface, we have to support for now.

if ! cargo run -- \
	"$fml_file" \
	android features \
	--output "$build_dir/Legacy.kt" \
	--channel release \
	--package com.foo \
	--classname FooNimbus \
	--r-package com.foo.app \
    2> /dev/null ;
then
	fail "Legacy 'android features' command, as used in NimbusGradlePlugin.grooovy"
fi

if ! cargo run -- \
	"$fml_file" \
	-o "$build_dir/Legacy.swift" \
	ios features \
	--classname FooNimbus \
	--channel release \
    2> /dev/null ;
then
	fail "Legacy 'ios features' command, as used in nimbus-fml.sh"
fi

if ! cargo run -- \
	"$fml_file" \
	-o "$build_dir/legacy-experimenter-ios.yaml" \
	experimenter \
	--channel release \
    2> /dev/null ;
then
	fail "Legacy 'experimenter' command, as used in numbus-fml.sh"
fi

if ! cargo run -- \
	"$fml_file" \
	experimenter \
	--output "$build_dir/legacy-experimenter-android.yaml" \
    2> /dev/null ;
then
	fail "Legacy 'experimenter' command, as used in NimbusGradlePlugin.grooovy"
fi

# Target command line interface

if ! cargo run -- generate \
	--channel release \
    --language kotlin \
	"$fml_file" \
	"$build_dir" \
    2> /dev/null ;
then
	fail "New style 'generate' with directory and explicit 'language' = kotlin"
fi

if ! cargo run -- generate \
	--channel release \
    --language swift \
	"$fml_file" \
	"$build_dir" \
    2> /dev/null ;
then
	fail "New style 'generate' with directory and explicit 'language' = swift"
fi

if ! cargo run -- generate \
	--channel release \
	"$fml_file" \
	"$build_dir/Implied.kt" \
    2> /dev/null ;
then
	fail "New style 'generate' with filename and implied language = kotlin"
fi

if ! cargo run -- generate \
	--channel release \
	"$fml_file" \
	"$build_dir/Implied.swift" \
    2> /dev/null ;
then
	fail "New style 'generate' with filename and implied language = swift"
fi

if ! cargo run -- generate-experimenter \
	"$fml_file" \
	"$build_dir/generate-experimenter.json" \
    2> /dev/null ;
then
	fail "New style 'generate-experimenter' with implied language = json"
fi

popd >/dev/null || exit 0
