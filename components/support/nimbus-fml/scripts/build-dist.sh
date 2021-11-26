#!/usr/bin/env bash

targets="aarch64-apple-darwin x86_64-unknown-linux-musl x86_64-apple-darwin"
dry_run=false

root_dir=$(dirname "$0")/../../../..
fml_dir=$root_dir/components/support/nimbus-fml
target_dir=$root_dir/target
filename=nimbus-fml
dist_file=${filename}.zip

prompt='$'
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

# Now we need to add a linker

zip_cmd="zip $(pwd)/$dist_file "

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

    zip_cmd="$zip_cmd $TARGET/release/$filename"

done
echo
echo "## Preparing dist archive"
if [[ $dry_run != "true" ]] ; then
    (cd "$target_dir" && $zip_cmd )
else
    echo "$prompt (cd $target_dir ; $zip_cmd )"
fi