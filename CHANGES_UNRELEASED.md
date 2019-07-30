**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.35.4...master)

## General

- For maintainers only: please delete the `libs/{desktop, ios, android}` folders and start over using `./build-all.sh [android|desktop|ios]`.

### What's fixed

- Android x86_64 crashes involving the `intel_aes_encrypt_cbc_128` missing symbol have been fixed.

## Push

### Breaking changes

- `PushManager.dispatchForChid` method has been renamed to `dispatchInfoForChid` and its result type is now Nullable. ([#1490](https://github.com/mozilla/application-services/pull/1490))
