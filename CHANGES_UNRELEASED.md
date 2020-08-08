**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v62.0.0...main)

## General

- Do not check total bytes quota on storage.sync.remote operations ([Bug 1656947](https://bugzilla.mozilla.org/1656947))

## FxA Client

### What's new ###
- Send-tab metrics are recorded. A new function, `fxa_gather_telemetry` on the
  account object (exposed as `account.gatherTelemetry()` to Kotlin) which
  returns a string of JSON.

  This JSON might grow to support non-sendtab telemetry in the future, but in
  this change it has:
  - `commands_sent`, an array of objects, each with `flow_id` and `stream_id`
    string values.
  - `commands_received`, an array of objects, each with `flow_id`, `stream_id`
    and `reason` string values.

  [#3308](https://github.com/mozilla/application-services/pull/3308/)
