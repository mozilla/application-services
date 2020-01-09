**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.48.1...master)

## Places

### Breaking Changes

- The Android bindings now collect some basic performance and quality metrics via Glean.
  Applications that submit telemetry via Glean must request a data review for these metrics
  before integrating the places component. See the component README.md for more details.
  ([#2431](https://github.com/mozilla/application-services/pull/2431))
  ([#2442](https://github.com/mozilla/application-services/pull/2442))
