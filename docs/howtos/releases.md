# Application Services Release Process

## Nightly builds

Nightly builds are automatically generated using a taskcluster cron task.

- The results of the latest successful nightly build is listed here:
  https://firefox-ci-tc.services.mozilla.com/tasks/index/project.application-services.v2.nightly/latest
- The latest nightly decision task should be listed here:
  https://firefox-ci-tc.services.mozilla.com/tasks/index/project.application-services.v2.branch.main.latest.taskgraph/decision-nightly
- If you don't see a decision task from the day before, then contact releng.  It's likely that the cron decision task is broken.

## Release builds

Release builds are generated from the `release-vXXX` branches.  Whenever a commit is pushed there,
we build all artifacts needed for the release.  Once we're ready to publish the release, we run the
taskcluster release promotion action which signs and publishes the artifacts.

TODO: explain release-promotion more.

## What to do at the end of a nightly cycle

1. Start this 2 workdays before the nightly cycle ends

2. Run the `automation/prepare-release.py` script.  This should:

 * Create a new branch named `release-vXXX`
 * Create a PR against that branch that updates `.buildconfig-android.yml` like this:

```
diff --git a/.buildconfig-android.yml b/.buildconfig-android.yml
index 8cd923873..6482018e0 100644
--- a/.buildconfig-android.yml
+++ b/.buildconfig-android.yml
@@ -1,4 +1,4 @@
-libraryVersion: 114.0a1
+libraryVersion: 114.0
 groupId: org.mozilla.appservices
```

 * Create a PR on `main` that starts a new CHANGELOG haeder.

3. Trigger release-promotion once the PRs are approved, merged, and CI has completed

 * TODO: explain how to do this

4. Tag the release with `automation/tag-release.py [major-version-number]`

5. Update consumer applications
  * firefox-android: Create a PR that updates
     `android-components/plugins/dependencies/src/main/java/ApplicationServices.kt` following the
      directions in the [release checklist](https://github.com/mozilla-mobile/firefox-android/blob/main/docs/contribute/release_checklist.md)
  * firefox-ios: TODO

### Cutting patch releases for uplifted changes

If you want to uplift changes into the a previous release:

* Make sure the changes are present in `main` and have been thoroughly tested.
* Checkout the `release-v114` branch
* Bump `libraryVersion` in `build-config-android.yml`
* Cherry-pick any commits that you want to uplift
* Create a PR for the changes
* Trigger release-promotion for the branch once the PRs are approved, merged, and CI has completed
