**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.40.0...master)

## General

### What's New

- Our components are now built with the newer Android NDK r20 instead of r15c. This change will make it easier for contributors to set up their development environment since there's no need to generate Android toolchains anymore. ([#1916](https://github.com/mozilla/application-services/pull/1916))  
For existing contributors, here's what you need to do immediately:
  - Download and extract the [Android NDK r20](https://developer.android.com/ndk/downloads).
  - Change the `ANDROID_NDK_ROOT` and `ANDROID_NDK_HOME` environment variables to point to the newer NDK dir. You can also delete the now un-used `ANDROID_NDK_TOOLCHAIN_DIR` variable.
  - Delete `.cargo/config` at the root of the repository if you have it.
  - Regenerate the Android libs: `cd libs && rm -rf android && ./build-all.sh android`.

## Logins

### What's new

- Added ability to get logins by hostname by using `LoginsStorage.getByHostname`. ([#1782](https://github.com/mozilla/application-services/pull/1782))
