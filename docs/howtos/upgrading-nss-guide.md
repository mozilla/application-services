# Guide to upgrading NSS

Our components rely on cryptographic primitives provided by [NSS](https://firefox-source-docs.mozilla.org/security/nss/index.html).
Every month or so, a new version of NSS is [published](https://firefox-source-docs.mozilla.org/security/nss/releases/index.html) and we should try to keep our version as up-to-date as possible.

Because it makes unit testing easier on Android, and helps startup performance on iOS, we compile NSS ourselves and link to it statically. Note that NSS is mainly used by Mozilla as a dynamic library and the NSS project is missing related CI jobs (iOS builds, windows cross-compile builds etc.) so you should expect breakage when updating the library (hence this guide).

---

## Updating the Version

The build code is located in the [`libs/`](https://github.com/mozilla/application-services/tree/main/libs) folder.

The version string is located in the beginning of [`build-all.sh`](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/libs/build-all.sh#L8-L11).

 For most NSS upgrades, you'll need to bump the version number in this file and update the downloaded archive checksum. Then follow the steps for [Updating the cross-compiled NSS Artifacts](#updating-the-cross-compiled-nss-artifacts) below. The actual build invocations are located in platform-specific script files (e.g. [`build-nss-ios.sh`](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/libs/build-nss-ios.sh)) but usually don't require any changes.

To test out updating NSS version:

* Ensure you've bumped the NSS in `build-all.sh`
* Clear any old NSS build artifacts: `rm -rf ./libs/desktop && cargo clean`
* Install the updates version: `./libs/verify-desktop-environment.sh`
* Try it out: `cargo test`

---

### Updating the Cross-Compiled NSS Artifacts

We use a Linux TC worker for cross-compiling NSS for iOS, Android and Linux desktop machines. However, due to the complexity of the NSS build process, there is no easy way for cross-compiling MacOS and Windows -- so we currently use pre-built artifacts for MacOS desktop machines (ref [#5210](https://github.com/mozilla/application-services/issues/5210)).

1. Look for the tagged version from the [NSS CI](https://treeherder.mozilla.org/jobs?repo=nss)
    > usually a description with something like _`Added tag NSS_3_90_RTM`_
2. Select the build for the following system(s) (first task with the title "B"):
    * For Intel MacOS: `mac opt-static`
3. Update [taskcluster/kinds/fetch/kind.yml](https://github.com/mozilla/application-services/blob/main/taskcluster/kinds/fetch/kind.yml), specifically `nss-artifact` task to the appropriate `url` and `checksum` and `size`
    > Note: _To get the checksum, you can run `shasum -a 256 {path-to-artifact}` or you can make a PR and see the output of the failed log._
4. Update the SHA256 value for darwin cross-compile in [libs/build-nss-desktop.sh](https://github.com/mozilla/application-services/blob/main/libs/build-nss-desktop.sh) to the same checksum as above.
5. Once the pull request lands, `build-nss-desktop.sh` should be updated once more using the L3 cache [Taskcluster artifact](https://firefox-ci-tc.services.mozilla.com/tasks/index/app-services.cache.level-3.content.v1.nss-artifact/latest).

---

## Exposing new functions

If the new version of NSS comes with new functions that you want to expose, you will need to:

* Add low-level bindings for those functions in the [`nss_sys` crate](
  ../../components/support/rc_crypto/nss/nss_sys); follow the instructions in
  README for that crate.
* Expose a safe wrapper API for the functions from the [`nss` crate](
  ../../components/support/rc_crypto/nss);
* Expose a convenient high-level API for the functions from the [`rc_crypto` crate](
  ../../components/support/rc_crypto);

## Tips for Fixing Bustage

On top of the primitives provided by NSS, we have built a safe Rust wrapper named [rc_crypto](https://github.com/mozilla/application-services/tree/main/components/support/rc_crypto) that links to NSS and makes these cryptographic primitives available to our components.

The linkage is done by the [`nss_build_common`](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/components/support/rc_crypto/nss/nss_build_common/src/lib.rs) crate. Note that it supports a `is_gecko` feature to link to NSS dynamically on Desktop.

Because the NSS static build process does not output a single `.a` file (it would be great if it did), this file must [describe](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/components/support/rc_crypto/nss/nss_build_common/src/lib.rs#L85-L133) for each architecture which modules should we link against. It is mostly a duplication of logic from the [NSS gyp build files](https://searchfox.org/nss/rev/d0ca572a63597a19889611c065273f131cc09b7a/lib/freebl/freebl.gyp#385-408). Note that this logic is also duplicated in our NSS lib build steps (e.g. [build-nss-desktop.sh](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/libs/build-nss-desktop.sh#L82-L114)).

One of the most common build failures we get when upgrading NSS comes from NSS adding new vectorized/asm versions of a crypto algorithm for a specific architecture in order to improve performance. This new optimized code gets implemented as a new gyp target/module that is emitted only for the supported architectures.
When we upgrade our copy of NSS we notice the linking step failing on CI jobs because of undefined symbols.

[This PR](https://github.com/mozilla/application-services/pull/2476) shows how we update `nss_common_build` and the build scripts to accommodate for these new modules. Checking the changelog for any suspect commit relating to hardware acceleration is rumored to help.
