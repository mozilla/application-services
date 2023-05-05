Here's how to upgrade the Android NDK version:

* Update the version number in:
  * `build.gradle` (search for `ndkVersion`)
  * `taskcluster/docker/linux/Dockerfile` (search for `ANDROID_NDK_VERSION`)
  * `components/support/rc_crypto/nss/nss_build_common/src/lib.rs` (search for `ANDROID_NDK_VERSION`)
* Update these docs by replacing the old version with the new one:
  * docs/building.md
  * docs/howtos/locally-building-jna.md
* These may need updating if the directory structure changed between this version and the last
  * `libs/build-android-common.sh`: ensure the paths to the various binaries are correct.
  * `components/support/rc_crypto/nss/nss_build_common/src/lib.rs`: search for
    `DARWIN_X86_64_LIB_DIR` and `LINUX_X86_64_LIB_DIR`, and ensure that both point to the correct lib directory containing
    `libclang_rt.builtins-x86_64-android.a`.
