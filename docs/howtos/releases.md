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

This is documented under Release Management/Release Process Checklist Documentation, see application-services steps under [Beta Merge Day steps](https://wiki.mozilla.org/Release_Management/Release_Process_Checklist_Documentation#The_following_tasks_need_to_be_performed_on_Merge_Day_at_the_start_of_the_Beta_cycle)

### Cutting patch releases for uplifted changes (dot-release)

If you want to uplift changes into a previous release:

* Make sure the changes are present in `main` and have been thoroughly tested
* Find the PR for the changes and add this comment: `@mergify backport release-vXXX`
* Find the Bugzilla bug with the changes and add an uplift request
    * Find the attacment corresponding to new PR created from the `@mergify` comment.
    * Click the "details" link
    * Set `approval-mozilla-beta` or `approval-mozilla-release` to `?`
    * Save the form
* Release management will then:
  * Arrange for the backport to be merged
  * Create a new Application Services release in Ship-It for the release branch. Promote & ship the release
  * Tag the release in the Application Services repo
* Notify the Application Services team in case there is a need to cut a new release of [rust-components-swift](https://github.com/mozilla/rust-components-swift)
* Notify any affected consumer applications teams.

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
