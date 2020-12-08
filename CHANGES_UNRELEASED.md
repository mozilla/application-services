**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

[Full Changelog](https://github.com/mozilla/application-services/compare/v67.1.0...main)

## General

### ⚠️ Breaking changes ⚠️

- The bundled version of Nimbus SDK has been updated to v0.6.3, which includes
  the following breaking changes:
  - Removed `NimbusClient.resetEnrollment`.
  - `NimbusClient.{updateExperiments, optInWithBranch, optOut, setGlobalUserParticipation}` now return a list of telemetry events.
    Consumers should forward these events to their telemetry system (e.g. via Glean).
  - Removed implicit fetch of experiments on first use of the database. Consumers now must
    call update_experiments explicitly in order to fetch experiments from the Remote Settings
    server.


### What's Changed

- The bundled version of Glean has been updated to v33.5.0.
- Various third-party dependencies have been updated to their latest versions.
