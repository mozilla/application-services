#!/bin/bash
dir=$(dirname "$0")
# shellcheck disable=SC1091
source "${dir}/../common.sh"

clear
display_comment "We need to do this in another process"
display_comment "We're going to start a server embedded in nimbus-cli"
waitrun "nimbus-cli start-server"