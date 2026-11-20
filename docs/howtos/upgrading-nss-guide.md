# Guide to upgrading NSS

Our components rely on cryptographic primitives provided by [NSS](https://firefox-source-docs.mozilla.org/security/nss/index.html).
Every month or so, a new version of NSS is [published](https://firefox-source-docs.mozilla.org/security/nss/releases/index.html) and we should try to keep our version as up-to-date as possible.

Because it makes unit testing easier on Android, and helps startup performance on iOS, we compile NSS ourselves and link to it statically. Note that NSS is mainly used by Mozilla as a dynamic library and the NSS project is missing related CI jobs (iOS builds, windows cross-compile builds etc.) so you should expect breakage when updating the library (hence this guide).

---

## Updating the Version

The build code is located in the [`libs/`](https://github.com/mozilla/application-services/tree/main/libs) folder.

The version string is located in the beginning of [`build-all.sh`](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/libs/build-all.sh#L8-L11).

 For most NSS upgrades, you'll need to bump the version number in this file and update the downloaded archive checksum. The actual build invocations are located in platform-specific script files (e.g. [`build-nss-ios.sh`](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/libs/build-nss-ios.sh)) but usually don't require any changes.

To test out updating NSS version:

* Ensure you've bumped the NSS in `build-all.sh`
* Clear any old NSS build artifacts: `rm -rf ./libs/desktop && cargo clean`
* Install the updates version: `./libs/verify-desktop-environment.sh`
* Try it out: `cargo test`

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

The NSS static build process does not output a single `.a` file, so the set of archives to link varies by architecture (for example, NSS adds a new gyp target when it adds a vectorized implementation of an algorithm). NSS's `build.sh --static` lists the archives it built for the target in `nss-static.pc`. [`libs/copy-nss-libs.sh`](https://github.com/mozilla/application-services/blob/main/libs/copy-nss-libs.sh) copies those archives and the `.pc` files into the dist, and `nss_build_common` links the same list, so new modules should not need changes on our side. If linking fails with undefined symbols anyway, [`libs/check-nss-libs.sh`](https://github.com/mozilla/application-services/blob/main/libs/check-nss-libs.sh), which runs at the end of each NSS build, reports any archive that was built but not copied and defines a missing symbol.
