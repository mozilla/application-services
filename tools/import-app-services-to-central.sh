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
moz-phab patch --apply-to=here --skip-dependencies --no-branch D263599
# unstub toolchains
moz-phab patch --apply-to=here --skip-dependencies --no-branch D274371

# vet nimbus, rc_crypto, ece, etc
# moz-phab patch --apply-to=here --skip-dependencies --no-branch D258722
# lint
# moz-phab patch --apply-to=here --no-branch D246875
# build config tweaks
moz-phab patch --apply-to=here --no-branch D280709

# Apply the final "patch" in the stack, which we do by abusing sed.
# This is mildly (hah!) fragile.

# Update Cargo.toml.
sed -e 's|third_party/application-services|third_party/application-services|' Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml

# [dependencies] is conveniently at the end of these toml files
printf 'mozilla-central-workspace-hack = { version = "0.1", features = ["megazord"], optional = true }\n' >> third_party/application-services/megazords/full/Cargo.toml
printf 'mozilla-central-workspace-hack = { version = "0.1", features = ["embedded-uniffi-bindgen"], optional = true }\n' >> third_party/application-services/tools/embedded-uniffi-bindgen/Cargo.toml
printf 'mozilla-central-workspace-hack = { version = "0.1", features = ["nimbus-fml"], optional = true }\n' >> third_party/application-services/components/support/nimbus-fml/Cargo.toml
# We need to update the crate-type for the megazord
sed -e 's|crate-type = \["cdylib"\]|crate-type = \["staticlib"\]|' third_party/application-services/megazords/full/Cargo.toml > third_party/application-services/megazords/full/Cargo.toml.tmp
mv third_party/application-services/megazords/full/Cargo.toml.tmp third_party/application-services/megazords/full/Cargo.toml
# [features] is conveniently at the end of this toml
printf 'megazord = []\nembedded-uniffi-bindgen = []\nnimbus-fml = []\n' >> build/workspace-hack/Cargo.toml
# and more hacks - sue me ;) In the short term these are more fragile in theory than practice.

# Add app-services crates to the workspace `members`, unless their dependencies can't be vendored.
sed -e 's|  "security/mls/mls_gk",|  "security/mls/mls_gk",\
  "third_party/application-services/tools/embedded-uniffi-bindgen",\
  "third_party/application-services/tools/uniffi-bindgen-library-mode",\
  "third_party/application-services/components/ads-client",\
  "third_party/application-services/components/autofill",\
  "third_party/application-services/components/context_id",\
  "third_party/application-services/components/crashtest",\
  "third_party/application-services/components/example",\
  "third_party/application-services/components/filter_adult",\
  "third_party/application-services/components/fxa-client",\
  "third_party/application-services/components/init_rust_components",\
  "third_party/application-services/components/logins",\
  "third_party/application-services/components/merino",\
  "third_party/application-services/components/places",\
  "third_party/application-services/components/push",\
  "third_party/application-services/components/relay",\
  "third_party/application-services/components/relevancy",\
  "third_party/application-services/components/remote_settings",\
  "third_party/application-services/components/search",\
  "third_party/application-services/components/suggest",\
  "third_party/application-services/components/support/error",\
  "third_party/application-services/components/support/find-places-db",\
  "third_party/application-services/components/support/firefox-versioning",\
  "third_party/application-services/components/support/guid",\
  "third_party/application-services/components/support/interrupt",\
  "third_party/application-services/components/support/jwcrypto",\
  "third_party/application-services/components/support/payload",\
  "third_party/application-services/components/support/nimbus-fml",\
  "third_party/application-services/components/support/rand_rccrypto",\
  "third_party/application-services/components/support/rate-limiter",\
  "third_party/application-services/components/support/restmail-client",\
  "third_party/application-services/components/support/rc_crypto",\
  "third_party/application-services/components/support/rc_crypto/nss",\
  "third_party/application-services/components/support/rc_crypto/nss/nss_build_common",\
  "third_party/application-services/components/support/rc_crypto/nss/nss_sys",\
  "third_party/application-services/components/support/rust-log-forwarder",\
  "third_party/application-services/components/support/sql",\
  "third_party/application-services/components/support/text-table",\
  "third_party/application-services/components/support/tracing",\
  "third_party/application-services/components/support/types",\
  "third_party/application-services/components/sync_manager",\
  "third_party/application-services/components/sync15",\
  "third_party/application-services/components/tabs",\
  "third_party/application-services/components/viaduct",\
  "third_party/application-services/components/webext-storage",\
  "third_party/application-services/components/webext-storage/ffi",\
  "third_party/application-services/megazords/full",|'\
  Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml


# Add app-services crates whose dependencies have vendoring issues to the `exclude` section.
#
# Disable complaints about the backtick usage below.
# shellcheck disable=SC2016
sed -e 's|  "intl/l10n/rust/l10nregistry-tests",|  "intl/l10n/rust/l10nregistry-tests",\
\
  # Exclude various app-services crates to avoid vendoring in their dependencies\
  # This disadvantage of this is that building these crates will require a separate `Cargo.lock` file and `target` directory.\
  # This is not ideal, but we are willing accept the trade-off for now.\
  #\
  # These depend on `hyper-tls` and we do not want to bring in the subdependencies, like `openssl`\
  "third_party/application-services/components/support/viaduct-hyper",\
  "third_party/application-services/components/support/viaduct-reqwest",\
  # Excluded because it depends on `hyper` and having it as top-level member would make\
  # `cargo vet` require it to be `safe-to-deploy`.  However, since we only use it as a dev-dependency, we want\
  # it to be `safe-to-run`.\
  "third_party/application-services/components/support/viaduct-dev",\
  # CLIs that depend on `viaduct-hyper`\
  "third_party/application-services/examples/autofill-utils/",\
  "third_party/application-services/examples/cli-support/",\
  "third_party/application-services/examples/example-cli/",\
  "third_party/application-services/examples/fxa-client/",\
  "third_party/application-services/examples/merino-cli/",\
  "third_party/application-services/examples/places-autocomplete/",\
  "third_party/application-services/examples/places-utils/",\
  "third_party/application-services/examples/push-livetest/",\
  "third_party/application-services/examples/relay-cli/",\
  "third_party/application-services/examples/relevancy-cli/",\
  "third_party/application-services/examples/remote-settings-cli/",\
  "third_party/application-services/examples/suggest-cli/",\
  "third_party/application-services/examples/sync-pass/",\
  "third_party/application-services/examples/tabs-sync/",\
  "third_party/application-services/examples/viaduct-cli/",\
  "third_party/application-services/components/example/cli",\
  "third_party/application-services/components/support/nimbus-cli",\
  "third_party/application-services/components/support/nimbus-fml",\
  # Excluded to avoid vendoring in `trybuild` and its subdependencies\
  "third_party/application-services/components/support/error/tests",\
  # Temporarily in excludes until we can land some nimbus fixes:\
  # * Remove unicode_segmentation dependency\
  # * Split off the `examples` code into a separate crate\
  "third_party/application-services/components/nimbus",\
  "third_party/application-services/megazords/full",\
  # Temporarily in excludes until everyone is on the same `ohttp`/`bhttp` version\
  "third_party/application-services/components/as-ohttp-client",\
  # Excluded because of the `viaduct-reqwest` dependency\
  "third_party/application-services/megazords/ios-rust",|'\
  Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml

# Update Cargo.lock
# cargo update -p gkrust-shared
# Downgrade some versions to avoid having to vet the newer ones
# cargo update -p expect-test --precise 1.4.1
# cargo update -p fragile --precise 2.0.0
# cargo update -p mockito --precise 0.31.0
# cargo update -p predicates --precise 3.1.3
# cargo update -p predicates-core --precise 1.0.6
# cargo update -p predicates-tree --precise 1.0.9

# update .gitignore to exclude directories created by excluded app-services crates
sed -e 's|/dom/webgpu/tests/cts/vendor/target/|/dom/webgpu/tests/cts/vendor/target/\
/third_party/application-services/**/target/\
/third_party/application-services/**/Cargo.lock|' \
  .gitignore > .gitignore.tmp
mv .gitignore.tmp .gitignore
# .hgignore needs the same, but with a leading `^` instead of `/`
sed -e 's|\^dom/webgpu/tests/cts/vendor/target/|^dom/webgpu/tests/cts/vendor/target/\
^third_party/application-services/.*/target/\
^third_party/application-services/.*/Cargo.lock|' \
  .hgignore > .hgignore.tmp
mv .hgignore.tmp .hgignore

# ohttp needs to switch from feature 'app-svc' to 'gecko'
sed -e 's|"app-svc"|"gecko"|' third_party/application-services/components/viaduct/Cargo.toml > third_party/application-services/components/viaduct/Cargo.toml.tmp
mv third_party/application-services/components/viaduct/Cargo.toml.tmp third_party/application-services/components/viaduct/Cargo.toml

git commit -a -m "Integrate app-services into the build system"

# Vendor everything in - `--force`` while we work though the vetting.
./mach vendor rust --force
git add -f third_party/rust/.
git commit -a -m "final vendor."

# panic=unwind
moz-phab patch --apply-to=here --skip-dependencies --no-branch D284982

echo "Done!"
