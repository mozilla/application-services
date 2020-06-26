#!/bin/bash
set -x -e -v

PATH="$MOZ_FETCHES_DIR/go/bin:$PATH"
export PATH


case "$1" in
    linux64)   GOOS=linux; GOARCH=amd64 ;;
    macos64)   GOOS=darwin; GOARCH=amd64 ;;
    windows64) GOOS=windows; GOARCH=amd64 ;;
    *)
        echo "Unknown architecture $1 not recognized in repack-go.sh" >&2
        exit 1
    ;;
esac

export GOOS
export GOARCH

echo $GOOS
echo $GOARCH
ls -l $MOZ_FETCHES_DIR

# make sure I'm in the right repo git repo clone here to be able to build
cd "$MOZ_FETCHES_DIR" || exit 1
go build .

STAGING_DIR="resource-monitor"
mv resource-monitor resource-monitor.tmp
mkdir "${STAGING_DIR}"

cp resource-monitor.tmp "${STAGING_DIR}"/resource-monitor

tar -acf "resource-monitor.tar.$COMPRESS_EXT" "${STAGING_DIR}"/
mkdir -p "$UPLOAD_DIR"
cp "resource-monitor.tar.$COMPRESS_EXT" "$UPLOAD_DIR"

