#!/bin/bash

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# This is for adding to the bootstrap.sh (or Makefile) for an iOS project, to be run in the root of the project.
#
# curl https://raw.githubusercontent.com/mozilla/application-services/main/components/nimbus/ios/scripts/bootstrap.sh | bash -s -- $NIMBUS_FML_FILE
#
# The argument NIMBUS_FML_FILE defaults to ./nimbus.fml.yaml. If not present on the filesystem, the script downloads a
# starter sample FML file.
#
# The `nimbus-fml.sh` script is downloaded, a starter configuration for that script is downloaded, and a local
# modifications of that configuration is downloaded.
#
# In addition, the `nimbus-fml.sh` and `nimbus-fml-configuration.local.sh` are added to `./gitignore`.
#
set -euo pipefail

CMDNAME=$(basename "$0")
AS_BASE="https://raw.githubusercontent.com/mozilla/application-services/main"
PATH_PREFIX="components/nimbus/ios/scripts"

FML_FILE="./nimbus.fml.yaml"
FML_DIR="./bin"

USAGE=$(cat <<HEREDOC
${CMDNAME} - Nimbus Feature Manifest Language bootstrap

This script downloads the FML download script, which in turn downloads
the nimbus-fml command line tool, and keeps it up to date.

Furthermore it adds to the .gitignore the script and the configuration local developers may want to
make.

Next steps: edit the downloaded nimbus-fml-configuration.sh to taste, and add ./nimbus-fml.sh --verbose as a
Build Phase in Xcode.

USAGE:
    ${CMDNAME} [OPTIONS] [FML_FILE]

FML_FILE: is the location of the top level FML file. If missing, a sample one is created for you.

OPTIONS:
    -d, --directory <DIR>   The directory where the script and its configuration lives. Defaults to ./bin
    -h, --help              Prints this message
    --verbose
HEREDOC
)

helptext() {
    echo "$USAGE"
}

# fail_trap is executed if an error occurs.
fail_trap() {
    local result=$1
    local line_number=$2
    echo "Error calling Nimbus FML bootstrap.sh at line ${line_number}"
    exit "$result"
}

#Stop execution on any error
trap 'fail_trap $? $LINENO' ERR

# Process the command line args.
while (( "$#" )); do
    case "$1" in
        -h|--help)
            helptext
            exit 0
            ;;
        -d|--directory)
            FML_DIR=$2
            shift 2
            ;;
        --verbose)
            set -x
            shift 1
            ;;
        --debug)
            AS_BASE=https://raw.githubusercontent.com/mozilla/application-services/baae0dadb735dc3c7f198eb7e8264d377d9c9f75
            shift 1
            ;;
        --) # end argument parsing
            shift
            break
            ;;
        --*=|-*) # unsupported flags
            echo "Error: Unsupported flag $1" >&2
            exit 1
            ;;
        *) # preserve positional arguments
            FML_FILE=$1
            shift
            ;;
    esac
done

BASE_URL="$AS_BASE/$PATH_PREFIX"

download_file() {
    local url_suffix="$1"
    local filename="$2"
    local url="${BASE_URL}/${url_suffix}"
    curl --fail "${url}" --output "${filename}"

    if [[ "${filename:(-3):3}" == ".sh" ]] ; then
        chmod +x "$filename"
    fi
}

download_once() {
    local url_suffix="$1"
    local filename="$2"
    if [[ ! -f "$filename" ]] ; then
        download_file "${url_suffix}" "$filename"
    fi
}

download_fresh_copy() {
    local url_suffix="$1"
    local filename="$2"
    download_file "${url_suffix}" "$filename"
    add_to_gitignore "$filename"
}

add_to_gitignore() {
    local filename="${1/\.\//}"
    local res
    if [[ ! -f ".gitignore" ]] ; then
        touch ".gitignore"
    fi
    res=$(grep -e "$filename" ".gitignore" || echo "")
    if [[ -z "$res" ]] ; then
        echo "$filename" >> ".gitignore"
    fi
}

mkdir -p "$FML_DIR"

# Always download a fresh copy of the nimbus-fml.sh file.
download_fresh_copy nimbus-fml.sh "$FML_DIR/nimbus-fml.sh"

# As a starter, download the nimbus-fml-configuration, nimbus-fml-configuration.local and the nimbus.sample.fml.yaml
download_once nimbus-fml-configuration.sh "$FML_DIR/nimbus-fml-configuration.sh"
download_once nimbus-fml-configuration.local.sh "$FML_DIR/nimbus-fml-configuration.local.sh"
add_to_gitignore "$FML_DIR/nimbus-fml-configuration.local.sh"

download_once nimbus.sample.fml.yaml "$FML_FILE"
