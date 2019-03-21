## Application Services Build and Publish Pipeline

This document provides an overview of the build-and-publish pipeline used to make our work
in this repo available to consuming applications. It's intended both to document the pipeline
for development and maintenance purposes, and to serve as a basic analysis of the integrity
protections that it offers (so you'll notice there are notes and open questions in place where
we haven't fully hashed out all those details).

The key points:

* We use [Cargo](https://github.com/rust-lang/cargo) for building and testing the core Rust code in isolation,
  [Gradle](https://gradle.org/) with [rust-android-gradle](https://github.com/mozilla/rust-android-gradle)
  for combining Rust and Kotlin code into Android components and running tests against them,
  and [Carthage](https://github.com/Carthage/Carthage) for combining Rust and Swift code into iOS components.
* [TaskCluster](../automation/taskcluster/README.md) runs on every pull-request, tag
  and push to master, to ensure Android artifacts build correctly and to execute their
  tests via gradle.
* [CircleCI](../.circleci/config.yml) runs on every branch, pull-request, and tag,
  to execute lint checks and automated tests at the rust level.
    * TODO: how do we run automated tests for the Swift wrapper code?
* Releases are made by [manually creating a new tag](./howtos/cut-a-new-release.md),
  which triggers various CI jobs:
    * [CircleCI](../.circleci/config.yml) is used to build an iOS binary release on every tag,
      and publish it as a GitHub release artifcact.
    * [TaskCluster](../automation/taskcluster/README.md) is used to:
        * Build an Android binary release.
        * Publish it to nalexander@'s personal bintray (although this will soon change to publish
           to https://maven.mozilla.org).
        * Upload symbols to [Socorro](https://wiki.mozilla.org/Socorro).
           * TODO: does this mean the symbols are only valid for debugging Android crashes, not on iOS?

For Android consumers these are the steps by which Application Services code becomes available,
and the integrity-protection mechanisms that apply at each step:

1. Code is developed in branches and lands on `master` via pull request.
    * GitHub branch protection prevents code being pushed to `master` without review.
      * TODO: we should consider requiring signed tags before merge to `master`.
    * CircleCI and TaskCluster run automated tests against the code, but do not have
      the ability to push modified code back to GitHub.
      * TODO: Or do they? Could we do more to guard against this?
2. Developers manually create a release tag from latest `master`.
    * TODO: what protections do we have around creating new tags?
3. TaskCluster checks out the tag, builds it for all target platforms, and runs automated tests.
    * These tasks run in a pre-build docker image, helping assure integrity of the build environment.
    * TODO: could this step check for signed tags as an additional integrity measure?
4. TaskCluster uploads built artifacts to bintray
    * Secret key for uploading to bintray is guaded by a TaskCluster scope that's only available
      to this task.
    * TODO: we're in the process of moving this to maven
    * TODO: could a malicious dev dependency from step (3) influence the build environment here?
    * TODO: talk about how TC's "chain of trust" might be useful here.
5. Consumers fetch the published artifacts from bintray.

For iOS consumers the corresponding steps are:

1. Code is developed in branches and lands on `master` via pull request, as above.
2. Developers manually create a release tag from latest `master`, as above.
3. CircleCI checks out the tag, builds it, and runs automated tests.
    * TODO: These tasks to `curl https://sh.rustup.rs | sh` and `pip install` and similar,
      is that unnecessarily risk?
    * TODO: could this step check for signed tags as an additional integrity measure?
4. CircleCI runs Carthage to assemble a zipfile of built frameworks.
    * TODO: could a malicious dev dependency from step (3) influence the build environment here?
5. CircleCI the [dpl](https://github.com/travis-ci/dpl) to publish to GitHub as a release artifact.
    * CircleCI config contains a github token with appropriate permissions.
    * TODO: who own this github token, and what permissions does it have?
6. Consumers fetch the published artifacts from GitHub.

