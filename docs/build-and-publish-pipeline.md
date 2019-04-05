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
  and [Carthage](https://github.com/Carthage/Carthage) driving [XCode](../xconfig)
  for combining Rust and Swift code into iOS components.
* [TaskCluster](../automation/taskcluster/README.md) runs on every pull-request, release,
  and push to master, to ensure Android artifacts build correctly and to execute their
  tests via gradle.
* [CircleCI](../.circleci/config.yml) runs on every branch, pull-request, and release,
  to execute lint checks and automated tests at the rust level.
  * We [do not currently run Swift tests in CI](https://github.com/mozilla/application-services/issues/607),
    but intend to run those using CircleCI as well.
* Releases are made by [manually creating a new release](./howtos/cut-a-new-release.md) via github,
  which triggers various CI jobs:
    * [CircleCI](../.circleci/config.yml) is used to build an iOS binary release on every release,
      and publish it as a GitHub release artifcact.
    * [TaskCluster](../automation/taskcluster/README.md) is used to:
        * Build an Android binary release.
        * Upload Android library symbols to [Socorro](https://wiki.mozilla.org/Socorro).
        * Publish it to the 'mozilla-appservices' organization on [bintray](https://bintray.com/),
          which mirrors it to [jcenter](https://bintray.com/bintray/jcenter).
           * (although this will soon change to publish to https://maven.mozilla.org).
    * There is also a manual step where we mirror artifacts from bintray to maven.mozilla.org,
      as a temporary measure until we can get automatic publishing set up correctly.

For Android consumers these are the steps by which Application Services code becomes available,
and the integrity-protection mechanisms that apply at each step:

1. Code is developed in branches and lands on `master` via pull request.
    * GitHub branch protection prevents code being pushed to `master` without review,
      and requires signed tags (but doesn't check *who* they're signed by).
    * CircleCI and TaskCluster run automated tests against the code, but do not have
      the ability to push modified code back to GitHub thanks to the above branch protection.
      * TaskCluster jobs do not run against PRs opened by the general public,
        only for PRs from repo collaborators.
2. Developers manually create a release from latest `master`.
    * The ability to create new releases is managed entirely via github's permission model.
3. TaskCluster checks out the release tag, builds it for all target platforms, and runs automated tests.
    * These tasks run in a pre-built docker image, helping assure integrity of the build environment.
    * TODO: could this step check for signed tags as an additional integrity measure?
5. TaskCluster uploads symbols to Socorro.
    * The access token for this is currently tied to @eoger's LDAP account.
5. TaskCluster uploads built artifacts to bintray
    * Secret key for uploading to bintray is provisioned via TaskCluster,
      guarded by a scope that's only available to this task.
    * TODO: we're in the process of [moving this to maven.mozilla.org](https://github.com/mozilla/application-services/issues/252).
    * TODO: could a malicious dev dependency from step (3) influence the build environment here?
    * TODO: talk about how TC's "chain of trust" might be useful here.
6. Bintray mirrors the built artifacts to [jcenter](https://bintray.com/bintray/jcenter).
    * TODO: as above, we're in the process of [moving this to maven.mozilla.org](https://github.com/mozilla/application-services/issues/252).
7. On request, our operations team [manually mirrors artifacts to maven.mozilla.org](https://bugzilla.mozilla.org/show_bug.cgi?id=1540775).
8. Consumers fetch the published artifacts from maven.mozilla.org.

For iOS consumers the corresponding steps are:

1. Code is developed in branches and lands on `master` via pull request, as above.
2. Developers manually create a release from latest `master`, as above.
3. CircleCI checks out the release tag, builds it, and runs automated tests.
    * TODO: These tasks bootstrap their build environment by fetcing software over https.
      could we do more to ensure the integrity of the build enviroment?
    * TODO: could this step check for signed tags as an additional integrity measure?
    * TODO: can we prevent these steps from being able to see the tokens used
      for publishing in subsequent steps?
4. CircleCI runs Carthage to assemble a zipfile of built frameworks.
    * TODO: could a malicious dev dependency from step (3) influence the build environment here?
5. CircleCI uses [dpl](https://github.com/travis-ci/dpl) to publish to GitHub as a release artifact.
    * CircleCI config contains a github token with appropriate permissions to add release artifacts.
    * TODO: this is currently a personal token generated by @eoger,
      [is there a better way to grant more limited permisions to CircleCI](https://github.com/mozilla/application-services/issues/871)?
6. Consumers fetch the published artifacts from GitHub during their build process,
   using Carthage.

It's worth noting that Carthage will *prefer* to use the built binary artifacts,
but will happily check out the tag and compile from source itself if such artifacts
are not available.
