**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.34.0...master)

## General

### Megazords

The long-awaited android megazord changes have arrived. This has a large number
of changes, many of them breaking:
([#1103](https://github.com/mozilla/application-services/pull/1103))

- Consumers who depend on network features of application-services, but
  which were not using a megazord, will no longer be able to use a legacy
  HTTP stack by default.

- Consumers who depend on network features and *do* use a megazord, can no
  longer initialize HTTP in the same call as the megazord.

- Both of these cases should import the `org.mozilla.appservices:httpconfig`
  package, and call `RustHttpConfig.setClient(lazy { /* client to use */ })`
  before calling functions which make HTTP requests.

- For custom megazord users, the name of your megazord is now always
  `mozilla.appservices.Megazord`. You no longer need to load it by reflection,
  since the swapped-out version always has the same name as your custom version.

- The reference-browser megazord has effectively been replaced by the
  full-megazord, which is also the megazord used by default

- The steps to swap-out a custom megazord have changed. The specific steps are
  slightly different in various cases, and we will file PRs to help make the
  transition.

- Substitution builds once again work, except for running unit tests against
  Rust code.

## FxA Client

### Breaking changes

- The `FirefoxAccount.destroyDevice` method has been removed in favor of the
  more general `FirefoxAccount.disconnect` method which will ensure a full
  disconnection by invalidating OAuth tokens and destroying the device record
  if it exists. ([#1397](https://github.com/mozilla/application-services/issues/1397))
- The `FirefoxAccount.disconnect` method has been added to the Swift bindings as well.
- The `FirefoxAccount.beginOAuthFlow` method will redirect to a content page that
  forces the user to connect to the last seen user email. To avoid this behavior,
  a new `FirefoxAccount` instance with a new persisted state must be created.
