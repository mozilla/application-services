# Rapid releases and moving towards a monorepo

* Status: proposed
* Deciders:
  * Sync team: bendk, tarikeshaq, lougeniaC64, ddurst, mhammond
  * Release management: rvandermeulen
  * Release Engineering: jlorenzo
  * Firefox Android:  csadilek
  * Firefox iOS: nish
  * Firefox Desktop: sylvestre

* Date: Dec 19, 2022
* Feedback deadline: Jan 16, 2022

## Context and Problem Statement

Application services currently has a long cycle time for code changes.  We make a release from main on an ad-hoc schedule, averaging about 3-4 releases a month.  Our consumer applications take our releases also on an ad-hoc basis, `firefox-android` takes our releases fairly frequently, `firefox-ios` less frequently, and desktop very infrequently.  This means that code changes take a relatively long time to be released to the public.  For example when the `v96.0.1` release was taken by `firefox-android`, the new commits for that release ranged from 3-18 days old.

There are two main reasons for our long cycles:
  - Our current releases process requires a good deal of developer involvement.  Normal releases take a couple hours, during which a developer needs to monitor the progress and perform several manual steps along the way.  If the unreleased code has breaking changes, then the developer also needs to merge the PR for the consumer application(s) after the release, which is not ideal since that developer is often not the one who wrote the PR.  Worse, if the dev who caused the breaking changes forgot to create a PR, they need to create that PR now, leading to multiple devs coordinating for the release.  Releases that are not from main also require [extra work](https://github.com/mozilla/application-services/blob/6ef3a2f2c4fef822a539e0fa375683c6acfb2617/docs/howtos/cut-a-new-release.md#make-a-new-point-release-from-an-existing-release-that-is-behind-latest-main): creating a branch for that version, backporting the fix, making a patch release, etc.
  - Both application-services team and the application teams generally put low priority on updating the application-services version.  We generally focus on the benefits of waiting to take releases rather than the costs.

Mozilla generally favors a rapid release model to long cycle times.  Firefox desktop adopted rapid releases over a decade ago, the mobile applications have a similar release cadence, and there is current work to move our services towards rapid releases.  Although there are some costs to rapid releases, as a company we've decided that we're willing to accept them.

Application services should consider moving towards a rapid release model.  The same reasoning that led other teams to adopt rapid releases also applies to us.  Furthermore, the mobile teams are working to increase their release cadence even further, which may depend on application-services also increasing ours.  Finally, we are in the process of moving towards a single monorepo for android code, with potential to add iOS code and/or move into the moz-central monorepo. This would require that application services code is taken by all other consumer applications in the monorepo as soon as it's merged.  Reducing the amount of time between our releases and when applications take those releases brings us closer to that monorepo world.

## Decision Drivers

* We want to get changes merged and released to our consumer applications faster.
* We want to simplify pushing new code to applications, especially urgent bug fixes.
* We want to move towards the future monorepo world.
* Mozilla is moving towards rapid releases for mobile code.
* We are willing to accept the overhead of upgrading application-services more frequently in application repositories.

## Considered Options

* **(A) Keep our current release process**
* **(B) Move to a rapid release process where changes in app-services are frequently merged into consumer applications**
    * Phase 1: update our branching model and release process (Jan 2023 - Feb 2023)
        * Update our branches to match how firefox-android works: a `main` branch that is used for the current nightly builds and a `releases_v[N]` for each previous release.
        * Automate a nightly release of our `main` branch
          * See the discussion at the bottom of this document for how this could work
          * Note: regardless of which option we choose, we will no longer be using semver-style versions and also not following semver semantics.  We will be releasing breaking changes in a nightly cycle without bumping the major version.
        * Manually handle patch releases from the `release_v*` branches
          * Update our version number to `[firefox-version].[N+1]`
          * Create a GitHub release from the branch
          * We should avoid having unreleased code in these branches.  Releases should be been soon after new code is committed to those branches.
          * Code should always be uplifted into these branches.  Before code is merged into the beta branch it should have already been merged into `main` and released as a nightly.  Before code is merged into `release_v[N-1]` branch, it should have merged and released in the `release_v[N]` branch.
        * Continue to maintain our CHANGELOG, but with new versioning.  In main, keep a rolling log of updates with the header `[firefox-version].0`
        * This replaces our current release process.
        * To push changes to consumers:
           * Create a PR in their repository that bumps the application-services version to one of these releases.
           * If there is an issue merging any PR, then:
             * Revert any PRs that were merged other applications.
             * Revert the breaking change in application-services
             * Wait until the next nightly release to merge again.
        * We will implement a script which will be manually run on the monthly Firefox merge date. This script will:
          * Create a `release_v[N]` branch of of main
          * Update the version number to be the final release version, e.g. change `v[N].0a1` to  `v[N].0`
          * Create a GitHub release for that branch
          * Bump the major version number in main and start a new changelog header.
    * Phase 2: Automate merges of `application-services` into consumer applications (March 2023 - May 2023)
      * Create a system for running the CI for Android, iOS, and Desktop against an in-progress application-service branch.  This is needed so that we can have confidence that the Android/iOS/Desktop code to resolve a breaking change is ready before we merge that breaking change into our main branch.
      * Every morning, for each consumer application (including all mobile apps and Desktop):
          * Create a PR to take the current application-services code in `main`.
          * Set up an auto-merge system to automatically merge the PRs.
          * As with phase 1, if there is a merge issue for any application, then back out the change for all applications that did get the merge.  This is why we start merges in the morning.
          * Set up an alert that gets sent to `sync-bots` if the test suite fails.  This means we would need to identify why the tests failed and assign a developer to resolve the issue.  The expectation would be that the developer fixes things in time for the next nightly merge.
          * Note: we currently have automation to do some of this for both iOS and Android.  We should leverage the existing code as much as possible.
      * Perform the same process whenever a new application-services version is published in a `release_v[N]` branch.
    * All application-services changes that are breaking for consumers should have corresponding PRs in the consumer repos that have been reviewed and approved.  If we accidentally merge the change into application-services before the PRs are ready, then we should back the change out.

## Decision Outcome

**(B) Move to a rapid release process where changes in app-services are frequently merged into consumer applications**

## Pros and Cons of the Options

### Keep our current release process

  * Good, because it doesn't require changes to our workflow
  * Bad, for the reasons enumerated at the top of this document

### Move to a rapid release process where changes in app-services are frequently merged into consumer applications

  * Good, because code will get merged and released more frequently.
  * Good, because we don't need to spend time cutting releases.
  * Good, because uplifts can be done much quicker.
  * Good, because more of our release process will be automated.
  * Bad, because we need to implement new automation.
  * Bad, because when breaking changes cause the automatic nightly merge to fail for an application.  This means a developer will get the relatively urgent task of resolving things.  However, this can be mitigated by the policy of not merging changes until we're sure they can be taken by all the consumer applications.
  * Good, because this avoids the current state of affairs where our `main` branch is not ready to be taken by the applications.  This means that a) our current code is not getting used and tested and b) it's very complicated to get other changes to that application, including bugfixes.
  * Bad, because breaking changes require the corresponding PRs to be approved from all other teams before they can be merged into application-services.
  * Good, because this is how things will work in a monorepo world.  Adjusting our workflow now prepares us for the future and will give us insights that may help guide the monorepo migration.
  * Good, because our version numbers will align more closely with the rest of Mozilla.
  * Good, because it makes it easier to know which application-services version the applications are using.  The current Firefox X release will always be using the latest application-services.X release -- for both the mobile apps and Desktop.
  * Good, because we align better with the Firefox process and which may make it easier to use Mozilla tools later on, for example Buildhub or Product Details.

# Non-normative discussion

## Impact on other teams
### Application teams -- Android, iOS, and Desktop

* Phase 1
  * The application-services version number style will change
  * There may be a increase in the amount of application-service version upgrade PRs to approve.  We'll want to make sure that each application is on an application-services release that matches their release (i.e. avoiding having Fenix 110.x on application-services 109.x).  For the Android/iOS teams this will only be a small increase, but for Desktop it will be larger.
* Phase 2
  * More application-services PRs to approve, especially at the start.  Eventually, we should be able to arrange it so most PRs get auto-merged.

### Release Management

* Will now handle tagging, releasing, and version bumping application-services at the end of the nightly cycle.
* Will now handle patch releases from the release / nightly branches

### Release Engineering

* Support the application-services team in developing the automation.  I believe most of the proposed changes are simple enough that application-services team can handle it ourselves.

## Nightly releases

The ADR mentions making nightly releases from the main branch.  There are two basic approaches for this:

  * **Create new releases**: Create a new version number, GitHub tag, GitHub release, Kotlin/swift packages, etc.
  * **Update a single release**.  Use a name like `[firefox-version].0a1` for the nightlies and don't change it.  Don't create GitHub tags or releases.  Re-publish the existing Kotlin/Swift packages rather than create new ones.

We plan to follow the second approach and use `[firefox-version].0a1` for our version number.  This follows how other teams manage their releases.  The main benefit of the first option is that it fits better with some of our existing tooling, but we expect to replace that with new tooling.
