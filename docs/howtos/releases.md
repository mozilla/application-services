# Application Services Release Process

## Nightly builds

Nightly builds are automatically generated using a taskcluster cron task.

- The results of the latest successful nightly build is listed here:
  https://firefox-ci-tc.services.mozilla.com/tasks/index/project.application-services.v2.nightly/latest
- The latest nightly decision task should be listed here:
  https://firefox-ci-tc.services.mozilla.com/tasks/index/project.application-services.v2.branch.main.latest.taskgraph/decision-nightly
- If you don't see a decision task from the day before, then contact releng.  It's likely that the cron decision task is broken.

## Release builds

Release builds are generated from the `release-vXXX` branches and published with the
`release-promotion` action, which has several phases:

- Whenever a commit is pushed to a release branch, we build candidate artifacts. These artifacts are
  shippable -- if we decide that the release is ready, they just need to be copied to the correct
  location.
- The `push` phase of `release-promotion` copies the candidate to a staging location where they can
  be tested.
- The `ship` phase of `release-promotion` copies the candidate to their final, published, location.

## What to do at the end of a nightly cycle

1. Start this 2 workdays before the nightly cycle ends

2. Run the `automation/prepare-release.py` script.  This should:

 * Create a new branch named `release-vXXX`
 * Create a PR against that branch that updates `version.txt` like this:

```diff
diff --git a/version.txt b/version.txt
index 8cd923873..6482018e0 100644
--- a/version.txt
+++ b/version.txt
@@ -1,4 +1,4 @@
-114.0a1
+114.0
```

 * Create a PR on `main` that starts a new CHANGELOG header.

3. Ask releng to trigger release-promotion once the PRs are approved, merged, and CI has completed

4. Tag the release with `automation/tag-release.py [major-version-number]`

5. Update consumer applications
  * firefox-android: Create a PR that updates
     `android-components/plugins/dependencies/src/main/java/ApplicationServices.kt` following the
      directions in the [release checklist](https://github.com/mozilla-mobile/firefox-android/blob/main/docs/contribute/release_checklist.md)
  * firefox-ios: TODO

### Cutting patch releases for uplifted changes

If you want to uplift changes into the a previous release:

* Make sure the changes are present in `main` and have been thoroughly tested.
* Checkout the `release-vXXX` branch, where `XXX` is the major version number.
* Bump the version in `version.txt`
* Cherry-pick any commits that you want to uplift
* Create a PR for the changes
* Trigger release-promotion for the branch once the PRs are approved, merged, and CI has completed


# What gets built in a release?

We build several artifacts for both nightlies and releases:
  - `nightly.json` / `release.json`.  This is a JSON file containing metadata from successful
    builds.  The metadata for the latest successful build can be found from a taskcluster index:
    https://firefox-ci-tc.services.mozilla.com/api/index/v1/task/project.application-services.v2.release.latest/artifacts/public%2Fbuild%2Frelease.json
    The JSON file contains:
    - The version number for the nightly/release
    - The git commit ID
    - The maven channel for Kotlin packages:
      - `maven-production`: https://maven.mozilla.org/?prefix=maven2/org/mozilla/appservices/
      - `maven-nightly-production`: https://maven.mozilla.org/?prefix=maven2/org/mozilla/appservices/nightly/
      - `maven-staging`: https://maven-default.stage.mozaws.net/?prefix=maven2/org/mozilla/appservices/
      - `maven-nightly-staging`: https://maven-default.stage.mozaws.net/?prefix=maven2/org/mozilla/appservices/nightly/
    - Links to `nimbus-fml.*`: used to build Firefox/Focus on Android and iOS
    - Links to `*RustComponentsSwift.xcframework.zip`: XCFramework archives used to build Firefox/Focus on iOS
    - Link to `swift-components.tar.xz`: UniFFI-generated swift files which get extracted into the
      `rust-components-swift` repository for each release.

## Nightly builds

For nightly builds, consumers get the artifacts directly from the taskcluster.

  - For, `firefox-android`, the nightlies are handled by [relbot](https://github.com/mozilla-mobile/relbot/)
  - For, `firefox-ios`, the nightlies are consumed by [rust-components-swift](https://github.com/mozilla/rust-components-swift).  `rust-components-swift` makes a github release, which is picked up by a Github action in [firefox-ios](https://github.com/mozilla-mobile/firefox-ios)

## Release promotion

For real releases, we use the taskcluster release-promotion action.  Release promotion happens in two phases:
  - `promote` copies the artifacts from taskcluster and moves them to a staging area.  This
    allows for testing the consumer apps with the artifacts.
  - `ship` copies the artifacts from the staging area to archive.mozilla.org, which serves as
    their permanent storage area.
