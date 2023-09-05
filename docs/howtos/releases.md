# Application Services Release Process

## Nightly builds

Nightly builds are automatically generated using a taskcluster cron task.

- The results of the latest successful nightly build is listed here:
  https://firefox-ci-tc.services.mozilla.com/tasks/index/project.application-services.v2.nightly/latest
- The latest nightly decision task should be listed here:
  https://firefox-ci-tc.services.mozilla.com/tasks/index/project.application-services.v2.branch.main.latest.taskgraph/decision-nightly
- If you don't see a decision task from the day before, then contact releng.  It's likely that the cron decision task is broken.

## Release builds

Release builds are generated from the `release-vXXX` branches and triggered in Ship-it

- Whenever a commit is pushed to a release branch, we build candidate artifacts. These artifacts are
  shippable -- if we decide that the release is ready, they just need to be copied to the correct
  location.
- The `push` phase of `release-promotion` copies the candidate to a staging location where they can
  be tested.
- The `ship` phase of `release-promotion` copies the candidate to their final, published, location.

## [Release management] Creating a new release
> This part is 100% covered by the Release Management team. The dev team should not perform these steps.

On Merge Day we take a snapshot of the current `main`, and prepare a release. See [Firefox Release Calendar](https://whattrainisitnow.com/calendar/).

1. Create a branch name with the format `releases-v[release_version]` off of the `main` branch (for example, `release-v118`) through the GitHub UI.
`[release_version]` should follow the Firefox release number. See [Firefox Release Calendar](https://whattrainisitnow.com/calendar/).

2. Create a PR against the release branch that updates `version.txt` and updates the `CHANGELOG.md` as follows:
  * In [version.txt](https://github.com/mozilla/application-services/blob/main/version.txt), update the version from [release_version].0a1 to [release_version].0. 
```diff
diff --git a/version.txt b/version.txt
--- a/version.txt
+++ b/version.txt
@@ -1 +1 @@
-118.0a1
+118.0
```
  * In [CHANGELOG.md](https://github.com/mozilla/application-services/blob/main/CHANGELOG.md), change `In progress` to `_YYY-MM-DD_` to match the Merge Day date. 
```diff
diff --git a/CHANGELOG.md b/CHANGELOG.md
--- a/CHANGELOG.md
+++ b/CHANGELOG.md
@@ -1,8 +1,7 @@
- v118.0 (In progress)
+# v118.0 (_2023-08-28_)
```
  * Create a commit named 'Cut release v[release_version].0` and a PR for this change.
  * See [example PR](https://github.com/mozilla/application-services/pull/5792)

3. Create a PR against the release branch that updates `version.txt` and updates the `CHANGELOG.md` as follows:
  * In [version.txt](https://github.com/mozilla/application-services/blob/main/version.txt), update the version from [previous_release_version].0a1 to [release_version].0. 
```diff
diff --git a/version.txt b/version.txt
--- a/version.txt
+++ b/version.txt
@@ -1 +1@@
-118.0a1
+119.0a1
```
  * In [CHANGELOG.md](https://github.com/mozilla/application-services/blob/main/CHANGELOG.md), change the in progress version from [previous_release_version].0a1 to [release_version].0, add a header for the previous release version, and add a URL to the previous release version change log.
``` diff
diff --git a/CHANGELOG.md b/CHANGELOG.md
--- a/CHANGELOG.md
+++ b/CHANGELOG.md
@@ -1,8 +1,7 @@
-# v118.0 (In progress)
+# v119.0 (In progress)

[Full Changelog](In progress)

+# v118.0 (_2023-08-28_)
@@ -34,6 +36,8 @@
+[Full Changelog](https://github.com/mozilla/application-services/compare/v117.0...v118.0)
+
# v117.0 (_2023-07-31_)
```
  * Create a commit named 'Start release v[release_version].0` and a PR for this change.
  * See [example PR](https://github.com/mozilla/application-services/pull/5793)

4. Once all of the above PRs have landed, create a new Application Services release in Ship-It.
  * Promote and Ship the release.

5. Tag the release in the Application Services repo.

6. Inform the Application Services team to cut a release of [rust-components-swift](https://github.com/mozilla/rust-components-swift)
  * The team will tag the repo and let you know the git hash to use when updating the consumer applications
  
8. Update consumer applications
  * firefox-android: Follow the directions in the [release checklist](https://mozac.org/contributing/release-checklist)
  * firefox-ios: Follow the directions in the [release checklist](https://github.com/mozilla-mobile/firefox-ios/wiki/Release-Checklist)


### [Release management] Creating a new release via scripts:
1. Run the `automation/prepare-release.py` script.  This should:
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

2. Tag the release with `automation/tag-release.py [major-version-number]`



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
