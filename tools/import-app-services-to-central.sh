#!/bin/bash
#
# This script is used to "import" the application-services into mozilla-central/firefox-main and arrange
# for a successful build.
#
# This script must be run with the current directory being a clean m-c/m-f/f-m/fml/ where you want patches applied.
#
# WARNING: This messes with git in the current directory.
# It DOES NOT check it is being run in a clean environment or on the correct branch.

# While this script lives in app-services for convenience, it uses git to fetch app-services.
# This script can be anywhere but it must be run with the correct cwd - see above.
#
# If this is successful, it will have created a number of git commits and should be able to build and run Fenix.
set -ex

if [[ ! -d "$(pwd)/toolkit/components" ]]; then
  echo "ERROR: This script must be run from the root of mozilla-central/mozilla-firefox/firefox-main/whatever-we-are-calling-it"
  exit 1
fi

# If this happens to be a jj repo things get strange
export MOZ_AVOID_JJ_VCS=1

# existing patches against m-c
# Bug 1981747 - Add `./mach setup-app-services`
# moz-phab patch --apply-to=here --skip-dependencies --no-branch D260481
# Bug 1981871 - Make `./mach rusttests` run some tests via cargo directly,
# moz-phab patch --apply-to=here --skip-dependencies --no-branch D260480
# enable `--with-appservices-in-tree` config option by default
# moz-phab patch --apply-to=here --skip-dependencies --no-branch D263599

# vendor more a-s
moz-phab patch --apply-to=here --skip-dependencies --no-branch D282538
# integrate into the build - final patch, children will come too.
# This now has the "vendor" and "vet" parts.
moz-phab patch --apply-to=here --no-branch D288590
# unstub toolchains
moz-phab patch --apply-to=here --skip-dependencies --no-branch D274371

# vet nimbus, rc_crypto, ece, etc
# moz-phab patch --apply-to=here --skip-dependencies --no-branch D258722
# lint
# moz-phab patch --apply-to=here --no-branch D246875
# build config tweaks
# moz-phab patch --apply-to=here --no-branch D280709

echo "Done!"
