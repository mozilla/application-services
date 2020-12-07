# Guide to upgrading NSS

Our components rely on cryptographic primitives provided by [NSS](https://developer.mozilla.org/docs/Mozilla/Projects/NSS).
Every month or so, a new version of NSS is [published](https://developer.mozilla.org/en-US/docs/Mozilla/Projects/NSS/NSS_Releases) and we should try to keep our version as up-to-date as possible.

Because it makes unit testing easier on Android, and helps startup performance on iOS, we compile NSS ourselves and link to it statically. Note that NSS is mainly used by Mozilla as a dynamic library and the NSS project is missing related CI jobs (iOS builds, windows cross-compile builds etc.) so you should expect breakage when updating the library (hence this guide).

The build code is located in the [`libs/`](https://github.com/mozilla/application-services/tree/main/libs) folder.  
The version string is located in the beginning of [`build-all.sh`](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/libs/build-all.sh#L8-L11). For most NSS upgrades, the only action needed is to bump the version number in this file and update the downloaded archive checksum.  The actual build invocations are located in platform-specific script files (e.g. [`build-nss-ios.sh`](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/libs/build-nss-ios.sh))

On top of that, we have built a safe Rust wrapper named [rc_crypto](https://github.com/mozilla/application-services/tree/main/components/support/rc_crypto) that links to NSS and makes these cryptographic primitives available to our components.

The linkage is done by the [`nss_build_common`](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/components/support/rc_crypto/nss/nss_build_common/src/lib.rs) crate. Note that it supports a `is_gecko` feature to link to NSS dynamically on Desktop.  
Because the NSS static build process does not output a single `.a` file (it would be great if it did), this file must [describe](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/components/support/rc_crypto/nss/nss_build_common/src/lib.rs#L85-L133) for each architecture which modules should we link against. It is mostly a duplication of logic from the [NSS gyp build files](https://searchfox.org/nss/rev/d0ca572a63597a19889611c065273f131cc09b7a/lib/freebl/freebl.gyp#385-408). Note that this logic is also duplicated in our NSS lib build steps (e.g. [build-nss-desktop.sh](https://github.com/mozilla/application-services/blob/b0b3daa6580d04906fc53e9e479e8bebb464cf78/libs/build-nss-desktop.sh#L82-L114)).

One of the most common build failures we get when upgrading NSS comes from NSS adding new vectorized/asm versions of a crypto algorithm for a specific architecture in order to improve performance. This new optimized code gets implemented as a new gyp target/module that is emitted only for the supported architectures.
When we upgrade our copy of NSS we notice the linking step failing on CI jobs because of undefined symbols.

[This PR](https://github.com/mozilla/application-services/pull/2476) shows how we update `nss_common_build` and the build scripts to accommodate for these new modules. Checking the changelog for any suspect commit relating to hardware acceleration is rumored to help.
