**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.7...main)

## FxA Client

### ⚠️ Breaking changes ⚠️

- Adds support for `entrypoint` in oauth flow APIs: consumers of `beginOauthFlow` and `beginPairingFlow` (`beginAuthentication` and `beginPairingAuthentication` in ios) are now ***required*** to pass an `entrypoint` argument that would be used for metrics. This puts the `beginOauthFlow` and `beginPairingFlow` APIs inline with other existing APIs, like `getManageAccountUrl`.  ([#3265](https://github.com/mozilla/application-services/pull/3265))