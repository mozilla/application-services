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

# XXX: make sure we're in the right repo to be able to build
cd "$MOZ_FETCHES_DIR"/moztaskmonitor || exit 1
go build .

tar -acf "moztaskmonitor.tar.xz" "${PWD}"/moztaskmonitor
mkdir -p "$UPLOAD_DIR"
cp "moztaskmonitor.tar.xz" "$UPLOAD_DIR"
