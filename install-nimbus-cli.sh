#!/usr/bin/env bash

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

: "${BINARY_NAME:=nimbus-cli}"
: "${TASKCLUSTER_HOST:=https://firefox-ci-tc.services.mozilla.com}"
: "${SHELL:=bash}"
: "${DEBUG:=false}"
: "${NIMBUS_INSTALL_DIR:=""}"
: "${TARGET:=""}"

# This is largely inspired by the MIT-licensed get-helm-3 install script
# https://github.com/helm/helm/blob/main/scripts/get-helm-3

HAS_CURL="$(type "curl" &> /dev/null && echo true || echo false)"
HAS_WGET="$(type "wget" &> /dev/null && echo true || echo false)"
HAS_UNZIP="$(type "unzip" &> /dev/null && echo true || echo false)"

USE_SUDO="false"
IS_UPGRADE="false"

# echoProgress displays a message to the user.
echoProgress() {
    echo "✅ ${*}"
}

# echoError displays an error message to the user.
echoError() {
    echo "❎ ${*}"
}

echoInfo() {
    # Double space because the `i` seems to eat the first space.
    echo "ℹ️  ${*}"
}

# initArch discovers the architecture for this system.
initArch() {
  ARCH=$(uname -m)
  case $ARCH in
    aarch64) ARCH="aarch64" ;;
    arm64) ARCH="aarch64" ;;
    x86_64) ARCH="x86_64" ;;
    amd64) ARCH="x86_64" ;;
  esac
}

# initOS discovers the operating system for this system.
initOS() {
  OS=$(uname|tr '[:upper:]' '[:lower:]')

  case "$OS" in
    # Minimalist GNU for Windows
    mingw*|cygwin*) OS='windows' ;;
    darwin*) OS='darwin' ;;
    linux*) OS='linux' ;;
  esac
}

# verifySupported checks that the os/arch combination is supported for
# binary builds, as well whether or not necessary tools are present.
verifySupported() {
  if [ "${HAS_CURL}" != "true" ] && [ "${HAS_WGET}" != "true" ]; then
    echoError "Either curl or wget is required to be installed and on the PATH"
    exit 1
  fi
  if [ "${HAS_UNZIP}" != "true" ]; then
    echoError "unzip is required to be installed and on the PATH"
    exit 1
  fi
  initTargetBinary
}

# initTargetBinary maps the ARCH/OS onto a zip file as built on taskcluster.
initTargetBinary() {
  if [ -n "$TARGET" ] ; then
    check_target "$TARGET"
    return
  fi
  local file=""
  case "$ARCH-$OS" in
    "x86_64-darwin") file="x86_64-apple-darwin" ;;
    "aarch64-darwin") file="aarch64-apple-darwin" ;;
    "x86_64-windows") file="x86_64-pc-windows-gnu" ;;
    "x86_64-linux") file="x86_64-unknown-linux-musl" ;;
    "aarch64-linux") file="aarch64-unknown-linux-gnu" ;;
    *)
      fail_target "$ARCH-$OS"
      ;;
  esac

  TARGET="$file"
}

check_target() {
  case "$TARGET" in
    "x86_64-apple-darwin" |\
    "aarch64-apple-darwin" |\
    "x86_64-pc-windows-gnu" |\
    "x86_64-unknown-linux-gnu" |\
    "x86_64-unknown-linux-musl" |\
    "aarch64-unknown-linux-gnu")
      ;;
    *)
      fail_target "$TARGET"
      ;;
  esac
}

fail_target() {
  echoError "No pre-built binary for $1."
  echo -e "   Available pre-built binaries are:"
  echo -e "      aarch64-apple-darwin"
  echo -e "      aarch64-unknown-linux-gnu"
  echo -e "      x86_64-apple-darwin"
  echo -e "      x86_64-pc-windows-gnu"
  echo -e "      x86_64-unknown-linux-gnu"
  echo -e "      x86_64-unknown-linux-musl"
  echo -e "   To build from source, go to https://experimenter.info/nimbus-cli/install"
  exit 1
}

initInstallDirectory() {
  local my_dir
  local my_path
  local existing

  if [[ "$NIMBUS_INSTALL_DIR" != "" ]] ; then
    # If the user has specified with an environment variable where to install
    # then use that.
    my_dir=$(cd "$NIMBUS_INSTALL_DIR" && pwd)
    echoProgress "Installing into $my_dir"
  elif [[ "$(which "$BINARY_NAME")" != "" ]] ; then
    existing=$(which "$BINARY_NAME")
    # If the user is already using a version of the tool, then use that.
    # Follow the softlinks to where the file actually sites.
    existing=$(readlink -f "$existing")
    my_dir=$(dirname "$existing")
    IS_UPGRADE="true"
  else
    # Otherwise, we need to find some place on the existing PATH.
    my_path=$(echo "$PATH" | tr ':' '\n')

    # Prefer, in order:
    #   1. the XDG binary directory (~/.local/bin)
    #   2. ~/bin
    #   3. /usr/local/bin
    if echo "${my_path}" | grep -q "$HOME/.local/bin" ; then
        my_dir="$HOME/.local/bin"
    elif echo "${my_path}" | grep -q "$HOME/bin" ; then
        my_dir="$HOME/bin"
    elif echo "${my_path}" | grep -q "/usr/local/bin" ; then
        my_dir="/usr/local/bin"
    else
        # If we can't find even a /usr/local/bin (unlikely)
        # then we'll create the directory, and add it to the
        # PATH, as well as adding it to the shell's . rc file.
        my_dir="$HOME/.local/bin"
        echoProgress "Adding $my_dir to PATH in .${SHELL}rc"
        echo "export PATH=$PATH:\"$my_dir\"" >> "${HOME}/.${SHELL}rc"
        export PATH=$PATH:"$my_dir"
    fi
  fi

  if [[ ! -d "$my_dir" ]] ; then
    echoProgress "Creating $my_dir"
    mkdir -p "$my_dir"
  fi

  # If the directory isn't writeable, then we'll need to use sudo.
  if [[ ! -w "$my_dir" ]] ; then
    USE_SUDO="true"
  fi

  NIMBUS_INSTALL_DIR="$my_dir"
}

# downloadFile downloads the latest binary package and also the checksum
# for that binary.
downloadFile() {
  FILENAME="$BINARY_NAME-$TARGET.zip"
  DOWNLOAD_URL="$TASKCLUSTER_HOST/api/index/v1/task/project.application-services.v2.${BINARY_NAME}.latest/artifacts/public/build/$FILENAME"
  NIMBUS_TMP_ROOT=$(mktemp -dt "$BINARY_NAME-installer-XXXXXX")
  NIMBUS_TMP_FILE="$NIMBUS_TMP_ROOT/$FILENAME"
  echoProgress "Downloading $DOWNLOAD_URL"
  if [ "${HAS_CURL}" == "true" ]; then
    curl -SsL "$DOWNLOAD_URL" -o "$NIMBUS_TMP_FILE"
  elif [ "${HAS_WGET}" == "true" ]; then
    wget -q -O "$NIMBUS_TMP_FILE" "$DOWNLOAD_URL"
  fi
}

cleanup() {
  if [[ -d "${NIMBUS_TMP_ROOT:-}" ]]; then
    echoProgress "Cleaning up installation directory"
    rm -rf "$NIMBUS_TMP_ROOT"
  fi
}

success() {
  echoProgress "Success!"
  if [[ "$IS_UPGRADE" == "true" ]] ; then
    echoInfo "To see What's New, visit: https://experimenter.info/nimbus-cli/whats-new"
  fi
}

# runs the given command as root (detects if we are root already)
runAsRoot() {
  if [ $EUID -ne 0 ] && [ "$USE_SUDO" = "true" ]; then
    sudo "${@}"
  else
    "${@}"
  fi
}

# installFile installs the nimbus-cli binary.
installFile() {
  NIMBUS_TMP="$NIMBUS_TMP_ROOT/unzipped"
  mkdir -p "$NIMBUS_TMP"
  unzip -qj "$NIMBUS_TMP_FILE" -d "$NIMBUS_TMP"
  local binary
  if [[ "$OS" == "windows" ]] ; then
    binary="${BINARY_NAME}.exe"
  else
    binary=$BINARY_NAME
  fi
  echoProgress "Preparing $binary for install"
  chmod +x "$NIMBUS_TMP/$binary"
  if [[ "$OS" == "darwin" ]] ; then
    # Remove the quarantine bit from the binary
    # to avoid the warning dialog
    # https://eclecticlight.co/2017/12/11/xattr-com-apple-quarantine-the-quarantine-flag/
    xattr -d com.apple.quarantine "$NIMBUS_TMP/$binary" 2>/dev/null && rc=$? || rc=$?
    if [[ "$rc" == "0" ]] ; then
      echoProgress Removed quarantine flag
    else
      echoInfo "Using for the first time may trigger a malicious software warning. Fix with: https://support.apple.com/en-us/guide/mac-help/mchleab3a043/mac"
    fi
  fi
  runAsRoot cp "$NIMBUS_TMP/$binary" "$NIMBUS_INSTALL_DIR/$binary"
  echoProgress "$binary installed into $NIMBUS_INSTALL_DIR"
}

# help provides possible cli installation arguments
help () {
  echo "install-nimbus-cli.sh"
  echo
  echo "Usage:"
  echo -e "\t./install-nimbus-cli.sh [OPTIONS]"
  echo
  echo "Accepted cli options are:"
  echo -e "\t--directory DIRECTORY    install into the given directory"
  echo -e "\t--host HOST              get from the given taskcluster host"
  echo -e "\t--binary|-B BINARY       the binary that is installed, leave blank to derive a default"
  echo -e "                           try: x86_64-unknown-linux-gnu, x86_64-unknown-linux-musl,"
  echo -e "                                x86_64-pc-windows-gnu, x86_64-apple-darwin, aarch64-apple-darwin,"
  echo -e "                                aarch64-unknown-linux-gnu"
  echo
  echo -e "\t--debug                  be verbose in output"
  echo -e "\t--help|-h                prints this help"
}

# fail_trap is executed if an error occurs.
fail_trap() {
  local result=$1
  local lineNum=$2
  cleanup
  if [ "$result" != "0" ]; then
    if [[ -n "$INPUT_ARGUMENTS" ]]; then
      echoError "Failed to install $BINARY_NAME with the arguments provided: $INPUT_ARGUMENTS, at line number $lineNum"
    else
      echoError "Failed to install $BINARY_NAME, at line number $lineNum"
    fi
    echo -e "\tFor support, go to https://experimenter.info/nimbus-cli/install"
  fi
  exit "$result"
}

#Stop execution on any error
trap 'fail_trap $? $LINENO' EXIT
set -e

# Set debug if desired
if [ "${DEBUG}" == "true" ]; then
  set -x
fi

# Parsing input arguments (if any)
export INPUT_ARGUMENTS="${*}"
set -u

while (( "$#" )); do
  case "$1" in
    '--host')
       TASKCLUSTER_HOST="$2"
       shift 2
       ;;
    '--directory')
       NIMBUS_INSTALL_DIR="$2"
       shift 2
       ;;
    '--binary'|'-B')
       TARGET="$2"
       shift 2
       ;;
    '--debug')
       DEBUG="true"
       set -x
       shift
       ;;
    '--help'|-h)
       help
       exit 0
       ;;
    *) exit 1
       ;;
  esac
done
set +u

initArch
initOS
verifySupported
downloadFile
initInstallDirectory
installFile
cleanup
success
