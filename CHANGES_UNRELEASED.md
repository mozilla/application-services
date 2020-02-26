**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v0.52.0...master)

## Megazords

### What's changed

- The fenix megazord is no more! ([#2565](https://github.com/mozilla/application-services/pull/2565))

  The full megazord should be used instead. The two are functionally identical,
  however this should reduce the configuration surface required to use the
  application-services code.

  An example PR showing the changes typically required for this is available
  here: https://github.com/MozillaReality/FirefoxReality/pull/2867. Please feel
  free to reach out if you have any issues.

## Sync

### What's fixed

- Rust sync code is now more robust in the face of corrupt meta/global
  records. ([#2688](https://github.com/mozilla/application-services/pull/2688))

## FxA Client

### What's changed

- The `ensureCapabilities` method will not perform any network requests if the
  given capabilities are already registered with the server.
  ([#2681](https://github.com/mozilla/application-services/pull/2681)).

### What's fixed

- Ensure an offline migration recovery succeeding does not happen multiple times.
  ([#2706](https://github.com/mozilla/application-services/pull/2706))

## Places

### What's fixed

- `storage::history::apply_observation` and `storage::bookmarks::update_bookmark`
  now flush pending origin and frecency updates. This fixes a bug where origins
  might be flushed at surprising times, like right after clearing history
  ([#2693](https://github.com/mozilla/application-services/issues/2693)).

## Push

### What's fixed

- `PushManager.dispatchInfoForChid` does not throw `KotlinNullPointerException` anymore if the method returned nothing. ([#2703](https://github.com/mozilla/application-services/issues/2703))

## Sync

### What's changed

- Fewer updates to the 'clients' collection will be made ([#2624](https://github.com/mozilla/application-services/issues/2624))

### What's fixed

- In v0.52.0 we reported some network related fixes. We lied. This time
  we promise they are actually fixed ([#2616](https://github.com/mozilla/application-services/issues/2616),
  [#2617](https://github.com/mozilla/application-services/issues/2617)
  [#2623](https://github.com/mozilla/application-services/issues/2623))
