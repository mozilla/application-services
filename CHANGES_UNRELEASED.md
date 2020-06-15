**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

## General

- Remove the node.js integration tests helper and removes node from the circleci environment. ([#3187](https://github.com/mozilla/application-services/pull/3187))
- Put `backtrace` behind a cargo feature. ([#3213](https://github.com/mozilla/application-services/pull/3213))
- Move sqlite dependency down from rc_cryto to nss_sys. ([#3198](https://github.com/mozilla/application-services/pull/3198))
- Adds jwe encryption in scoped_keys. ([#3195](https://github.com/mozilla/application-services/pull/3195))
- Adds an implementation for [pbkdf2](https://www.ietf.org/rfc/rfc2898.txt). ([#3193](https://github.com/mozilla/application-services/pull/3193))
- Fix bug to correctly return the given defaults when the storageArea's `get()` method is used with an empty store ([bug 1645598](https://bugzilla.mozilla.org/show_bug.cgi?id=1645598)). ([#3236](https://github.com/mozilla/application-services/pull/3236))

## Android

- From now on the project uses the Android SDK manager side-by-side NDK. ([#3222](https://github.com/mozilla/application-services/pull/3222))
  - Download the new NDK by running `./verify-android-environment.sh` in the `libs` directory.
    - Alternatively, you can download the NDK in Android Studio by going in Tools > SDK Manager > SDK Tools > NDK (check Show Package Details) and check `21.3.6528147`.
  - The `ANDROID_NDK_ROOT`, `ANDROID_NDK_HOME` environment variables (and the directory they point to) can be removed.

[Full Changelog](https://github.com/mozilla/application-services/compare/v60.0.0...master)
