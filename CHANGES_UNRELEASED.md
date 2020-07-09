**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v61.0.7...main)

## FxA Client

### What's new ###
- Consumers can now optionally include parameters for metrics in `beginOAuthFlow` and `beginPairingFlow` (`beginAuthentication` and `beginPairingAuthentication` in ios). Those parameters can be passed in using a `MetricsParams` struct/class. `MetricsParams` is defined in both the Kotlin and Swift bindings. The parameters are the following ([#3328](https://github.com/mozilla/application-services/pull/3328)):
  - flow_id
  - flow_begin_time
  - device_id
  - utm_source
  - utm_content
  - utm_medium
  - utm_term
  - utm_campaign
  - entrypoint_experiment
  - entrypoint_variation

### ⚠️ Breaking changes ⚠️

- Adds support for `entrypoint` in oauth flow APIs: consumers of `beginOAuthFlow` and `beginPairingFlow` (`beginAuthentication` and `beginPairingAuthentication` in ios) are now ***required*** to pass an `entrypoint` argument that would be used for metrics. This puts the `beginOAuthFlow` and `beginPairingFlow` APIs inline with other existing APIs, like `getManageAccountUrl`.  ([#3265](https://github.com/mozilla/application-services/pull/3265))
- Changes the `authorizeOAuthCode` API to now accept an `AuthorizationParams` object instead of the individual parameters. The `AuthorizationParams` also includes optional `AuthorizationPKCEParams` that contain the `codeChallenge`, `codeChallengeMethod`. `AuthorizationParams` also includes an optional `keysJwk` for requesting keys ([#3264](https://github.com/mozilla/application-services/pull/3264))