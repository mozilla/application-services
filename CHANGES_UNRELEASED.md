**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v63.0.0...main)

## iOS

### ⚠️ Breaking changes ⚠️

- The `MozillaAppServices.framework` is now built using Xcode 12, so consumers will need
  update their own build accordingly.
  ([#3586](https://github.com/mozilla/application-services/pull/3586))

### What's changed

- The bundled version of glean has been updated to v32.4.0.
  ([#3590](https://github.com/mozilla/application-services/pull/3590))
  ()

## FxA Client

### What's changed

- Added a circuit-breaker to the `check_authorization_status` method.
  In specific circumstances, it was in possible to trigger a failure-recovery infinite loop,
  which will now error out after a certain now of retries.
  ([#3585](https://github.com/mozilla/application-services/pull/3585))

## Autofill

### What's changed ###
- Added a basic API and database layer for the autofill component. ([#3582](https://github.com/mozilla/application-services/pull/3582))

## Places

### What's changed
- Removed the duplicate Timestamp logic from Places, which now exists in Support, and updated the references.
  ([#3593](https://github.com/mozilla/application-services/pull/3593))
- Fixed a bug in bookmarks reconciliation that could lead to deleted items being resurrected
  in some circumstances.
  ([#3510](https://github.com/mozilla/application-services/pull/3510),
  [Bug 1635859](https://bugzilla.mozilla.org/show_bug.cgi?id=1635859))


## Support Code

### What's new

- The `rc_crypto` crate now supports ECDSA P384-SHA384 signature verification.
  ([#3557](https://github.com/mozilla/application-services/pull/3557))
