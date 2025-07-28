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
# gradle
moz-phab patch --apply-to=here --no-branch D245762
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

cd ..

mkdir -p services/app-services
cp -r tmp-app-services/* services/app-services
cp -r tmp-app-services/.buildconfig-android.yml services/app-services

rm -rf tmp-app-services

git add services/app-services
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

./mach vendor rust
# This will create a commit with Cargo.lock changing just for these crates, and many `third_party/rust` directories removed.
git commit -a -m "Re-vendor application-services from its new in-tree home"

# apply the final "patch" in the stack, which we do by abusing sed.
# This is mildly (hah!) fragile.

# [dependencies] is conveniently at the end of these toml files
printf 'mozilla-central-workspace-hack = { version = "0.1", features = ["megazord"], optional = true }\n' >> services/app-services/megazords/full/Cargo.toml
printf 'mozilla-central-workspace-hack = { version = "0.1", features = ["embedded-uniffi-bindgen"], optional = true }\n' >> services/app-services/tools/embedded-uniffi-bindgen/Cargo.toml
# We need to update the crate-type for the megazord
sed -e 's|crate-type = \["cdylib"\]|crate-type = \["staticlib"\]|' services/app-services/megazords/full/Cargo.toml > services/app-services/megazords/full/Cargo.toml.tmp
mv services/app-services/megazords/full/Cargo.toml.tmp services/app-services/megazords/full/Cargo.toml
# [features] is conveniently at the end of this toml
printf 'megazord = []\nembedded-uniffi-bindgen = []\n' >> build/workspace-hack/Cargo.toml
# and more hacks - sue me ;) In the short term these are more fragile in theory than practice.

# Add the 2 crates to the workspace which have binary targets
sed -e 's|  "security/mls/mls_gk",|  "security/mls/mls_gk",\
  "services/app-services/megazords/full",\
  "services/app-services/tools/embedded-uniffi-bindgen",|' \
  Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml

# exclude all the app-services crates.
sed -e 's|  "intl/l10n/rust/l10nregistry-tests",|  "intl/l10n/rust/l10nregistry-tests",\
\
  # app-services excluded members, also to avoid dev dependencies.\
  "services/app-services/components/autofill",\
  "services/app-services/components/fxa-client",\
  "services/app-services/components/logins",\
  "services/app-services/components/nimbus",\
  "services/app-services/components/places",\
  "services/app-services/components/push",\
  "services/app-services/components/relay",\
  "services/app-services/components/relevancy",\
  "services/app-services/components/remote_settings",\
  "services/app-services/components/search",\
  "services/app-services/components/suggest",\
  "services/app-services/components/support/error",\
  "services/app-services/components/support/guid",\
  "services/app-services/components/support/sql",\
  "services/app-services/components/support/tracing",\
  "services/app-services/components/sync15",\
  "services/app-services/components/tabs",\
  "services/app-services/components/webext-storage",\
  # app-services excluded members, for other reasons.\
  "services/app-services/megazords/ios-rust",|' \
  Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml

# Cargo.lock
cargo update -p gkrust-shared
git commit -a -m "Integrate app-services into the build system"

# XXX - Final vendor for rc_crypto, which doesn't yet `vet` - todo
# once it vets, we can just do a single vendor with the above one at the end.
./mach vendor rust --force --ignore-modified
git commit -a -m "final vendor of rc_crypto ignoring vet issues."

echo "Done!"
