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
moz-phab patch --apply-to=here --skip-dependencies --no-branch D260481
# Bug 1981871 - Make `./mach rusttests` run some tests via cargo directly,
moz-phab patch --apply-to=here --skip-dependencies --no-branch D260480
# gradle
moz-phab patch --apply-to=here --skip-dependencies --no-branch D245762
# enable `--with-appservices-in-tree` config option by default
moz-phab patch --apply-to=here --skip-dependencies --no-branch D263599
# vet `ece`
moz-phab patch --apply-to=here --skip-dependencies --no-branch D265023
# various hacks needed until we fix other things
moz-phab patch --apply-to=here --skip-dependencies --no-branch D265174
#lint
moz-phab patch --apply-to=here --no-branch D246875

# the import of app-services via a branch markh is maintaining while we work towards `main`
git clone https://github.com/mozilla/application-services tmp-app-services
cd tmp-app-services
git co monorepo

# xxx - this show-ref confused me - why doesn't `HEAD` work in place of refs/heads/monorepo??
commit=$(git show-ref refs/heads/monorepo | awk '{print $1}')

rm -rf .git .github
rm CHANGELOG.md CODE_OF_CONDUCT.md LICENSE README.md COPYRIGHT version.txt
rm Cargo.lock Cargo.toml clippy.toml
rm gradlew gradlew.bat build.gradle gradle.properties
rm -rf gradle
rm proguard-rules-consumer-jna.pro
rm install-nimbus-cli.sh
rm rust-toolchain.toml
rm -rf components/external
rm -rf docs/shared
# need a story for these generated deps - bug 1963617
rm DEPENDENCIES.md megazords/full/android/dependency-licenses.xml megazords/full/DEPENDENCIES.md megazords/ios-rust/DEPENDENCIES.md megazords/ios-rust/focus/DEPENDENCIES.md
# No Taskcluster for now, testing etc should come for free (or need tweaks to add the new components etc?)
rm -rf taskcluster/app_services_taskgraph taskcluster
# nimbus-gradle-plugin isn't needed
rm -rf tools/nimbus-gradle-plugin

cd ..

mkdir -p services/app-services
cp -r tmp-app-services/* services/app-services
cp -r tmp-app-services/.buildconfig-android.yml services/app-services

rm -rf tmp-app-services

# explicit "add -f" used to avoid throughout because local .gitignore might exclude stuff, eg, ".*"
git add -f services/app-services
git commit -m "Import application-services commit $commit"

# We've committed an app-services unmodified apart from removal of things we don't need.
# Update Cargo.toml and re-vendor.
sed -e 's|context_id = { git = .*$|context_id = { path = "services/app-services/components/context_id" }|' \
    -e 's|error-support = { git = .*$|error-support = { path = "services/app-services/components/support/error" }|' \
    -e 's|filter_adult = { git = .*$|filter_adult = { path = "services/app-services/components/filter_adult" }|' \
    -e 's|interrupt-support = { git = .*$|interrupt-support = { path = "services/app-services/components/support/interrupt" }|' \
    -e 's|relevancy = { git = .*$|relevancy = { path = "services/app-services/components/relevancy" }|' \
    -e 's|search = { git = .*$|search = { path = "services/app-services/components/search" }|' \
    -e 's|sql-support = { git = .*$|sql-support = { path = "services/app-services/components/support/sql" }|' \
    -e 's|suggest = { git = .*$|suggest = { path = "services/app-services/components/suggest" }|' \
    -e 's|sync15 = { git = .*$|sync15 = { path = "services/app-services/components/sync15" }|' \
    -e 's|tabs = { git = .*$|tabs = { path = "services/app-services/components/tabs" }|' \
    -e 's|tracing-support = { git = .*$|tracing-support = { path = "services/app-services/components/support/tracing" }|' \
    -e 's|viaduct = { git = .*$|viaduct = { path = "services/app-services/components/viaduct" }|' \
    -e 's|webext-storage = { git = .*$|webext-storage = { path = "services/app-services/components/webext-storage" }|' \
    Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml

# apply the final "patch" in the stack, which we do by abusing sed.
# This is mildly (hah!) fragile.

# [dependencies] is conveniently at the end of these toml files
printf 'mozilla-central-workspace-hack = { version = "0.1", features = ["megazord"], optional = true }\n' >> services/app-services/megazords/full/Cargo.toml
printf 'mozilla-central-workspace-hack = { version = "0.1", features = ["embedded-uniffi-bindgen"], optional = true }\n' >> services/app-services/tools/embedded-uniffi-bindgen/Cargo.toml
printf 'mozilla-central-workspace-hack = { version = "0.1", features = ["nimbus-fml"], optional = true }\n' >> services/app-services/components/support/nimbus-fml/Cargo.toml
# We need to update the crate-type for the megazord
sed -e 's|crate-type = \["cdylib"\]|crate-type = \["staticlib"\]|' services/app-services/megazords/full/Cargo.toml > services/app-services/megazords/full/Cargo.toml.tmp
mv services/app-services/megazords/full/Cargo.toml.tmp services/app-services/megazords/full/Cargo.toml
# [features] is conveniently at the end of this toml
printf 'megazord = []\nembedded-uniffi-bindgen = []\nnimbus-fml = []\n' >> build/workspace-hack/Cargo.toml
# and more hacks - sue me ;) In the short term these are more fragile in theory than practice.

# Add app-services crates to the workspace `members`, unless their dependencies can't be vendored.
sed -e 's|  "security/mls/mls_gk",|  "security/mls/mls_gk",\
  "services/app-services/tools/embedded-uniffi-bindgen",\
  "services/app-services/components/ads-client", \
  "services/app-services/components/autofill", \
  "services/app-services/components/context_id", \
  "services/app-services/components/crashtest", \
  "services/app-services/components/example", \
  "services/app-services/components/filter_adult", \
  "services/app-services/components/fxa-client", \
  "services/app-services/components/init_rust_components", \
  "services/app-services/components/logins", \
  "services/app-services/components/merino", \
  "services/app-services/components/places", \
  "services/app-services/components/push", \
  "services/app-services/components/relay", \
  "services/app-services/components/relevancy", \
  "services/app-services/components/remote_settings", \
  "services/app-services/components/search", \
  "services/app-services/components/suggest", \
  "services/app-services/components/support/error", \
  "services/app-services/components/support/find-places-db", \
  "services/app-services/components/support/firefox-versioning", \
  "services/app-services/components/support/guid", \
  "services/app-services/components/support/interrupt", \
  "services/app-services/components/support/jwcrypto", \
  "services/app-services/components/support/payload", \
  "services/app-services/components/support/rand_rccrypto", \
  "services/app-services/components/support/rate-limiter", \
  "services/app-services/components/support/restmail-client", \
  "services/app-services/components/support/rc_crypto", \
  "services/app-services/components/support/rc_crypto/nss", \
  "services/app-services/components/support/rc_crypto/nss/nss_build_common", \
  "services/app-services/components/support/rc_crypto/nss/nss_sys", \
  "services/app-services/components/support/rust-log-forwarder", \
  "services/app-services/components/support/sql", \
  "services/app-services/components/support/text-table", \
  "services/app-services/components/support/tracing", \
  "services/app-services/components/support/types", \
  "services/app-services/components/sync_manager", \
  "services/app-services/components/sync15", \
  "services/app-services/components/tabs", \
  "services/app-services/components/viaduct", \
  "services/app-services/components/webext-storage", \
  "services/app-services/components/webext-storage/ffi",|' \
  Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml


# Add app-services crates whose dependencies have vendoring issues to the `exclude` section.
#
# Disable complaints about the backtick usage below.
# shellcheck disable=SC2016
sed -e 's|  "intl/l10n/rust/l10nregistry-tests",|  "intl/l10n/rust/l10nregistry-tests",\
\
  # Exclude various app-services crates to avoid vendoring in their dependencies \
  # This disadvantage of this is that building these crates will require a separate `Cargo.lock` file and `target` directory. \
  # This is not ideal, but we are willing accept the trade-off for now. \
  #\
  # These depend on `hyper-tls` and we do not want to bring in the subdependencies, like `openssl` \
  "services/app-services/components/support/viaduct-hyper", \
  "services/app-services/components/support/viaduct-reqwest", \
  # Excluded because it depends on `hyper` and having it as top-level member would make \
  # `cargo vet` require it to be `safe-to-deploy`.  However, since we only use it as a dev-dependency, we want \
  # it to be `safe-to-run`. \
  "services/app-services/components/support/viaduct-dev", \
  # CLIs that depend on `viaduct-hyper` \
  "services/app-services/examples/autofill-utils/", \
  "services/app-services/examples/cli-support/", \
  "services/app-services/examples/example-cli/", \
  "services/app-services/examples/fxa-client/", \
  "services/app-services/examples/merino-cli/", \
  "services/app-services/examples/places-autocomplete/", \
  "services/app-services/examples/places-utils/", \
  "services/app-services/examples/push-livetest/", \
  "services/app-services/examples/relay-cli/", \
  "services/app-services/examples/relevancy-cli/", \
  "services/app-services/examples/remote-settings-cli/", \
  "services/app-services/examples/suggest-cli/", \
  "services/app-services/examples/sync-pass/", \
  "services/app-services/examples/tabs-sync/", \
  "services/app-services/examples/viaduct-cli/", \
  "services/app-services/components/example/cli", \
  "services/app-services/components/support/nimbus-cli", \
  "services/app-services/components/support/nimbus-fml", \
  # Excluded to avoid vendoring in `trybuild` and its subdependencies \
  "services/app-services/components/support/error/tests", \
  # Temporarily in excludes until we can land some nimbus fixes: \
  # * Remove unicode_segmentation dependency \
  # * Split off the `examples` code into a separate crate \
  "services/app-services/components/nimbus", \
  "services/app-services/megazords/full", \
  # Temporarily in excludes until everyone is on the same `ohttp`/`bhttp` version \
  "services/app-services/components/as-ohttp-client", \
  # Excluded because of the `viaduct-reqwest` dependency \
  "services/app-services/megazords/ios-rust",|' \
  Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml

# Update Cargo.lock
cargo update -p gkrust-shared
# Downgrade some versions to avoid having to vet the newer ones
cargo update -p expect-test --precise 1.4.1
cargo update -p fragile --precise 2.0.0
cargo update -p mockito --precise 0.31.0
cargo update -p predicates --precise 3.0.4
cargo update -p predicates-core --precise 1.0.6
cargo update -p predicates-tree --precise 1.0.9

# update .gitignore to exclude directories created by excluded app-services crates
sed -e 's|/dom/webgpu/tests/cts/vendor/target/|/dom/webgpu/tests/cts/vendor/target/\
/services/app-services/**/target/\
/services/app-services/**/Cargo.lock|' \
  .gitignore > .gitignore.tmp
mv .gitignore.tmp .gitignore

git commit -a -m "Integrate app-services into the build system"

# Vendor everything in
./mach vendor rust
git add -f third_party/rust/.
git commit -a -m "final vendor."

echo "Done!"
