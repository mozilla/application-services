# Branch builds

Branch builds are a way to build and test Fenix using branches from `application-services` and `firefox-android`.
iOS is not currently supported, although we may add it in the future (see [#4966](https://github.com/mozilla/application-services/issues/4966)).

## Breaking changes in an application-services branch.

 When we make breaking changes in an application-services branch, we typically make corresponding changes in an
 `android-components` branch.  Branch builds allow combining those branches together in order to run CI tests
 and to produce APKs for manual testing.  To trigger a branch build for this:

  - Create the PR for the `application-services` branch you're working on
  - Add `[firefox-android: branch-name]` to the PR title
  - The branch build tasks will be listed as checks the Github PR.  In particular:
    - `branch-build-fenix-test` and `branch-build-ac-test` will run the unit android-components/fenix unit tests
    - `branch-build-fenix-build` will contain the Fenix APK.

## Application-services nightlies

When we make non-breaking changes, we typically merge them into main and let them sit there until the next release. In
order to check that the current main really does only have non-breaking changes, we run a nightly branch build from the
`main` branch of `application-services`,

- To view the latest branch builds:
   - Open the [latest decision task](https://firefox-ci-tc.services.mozilla.com/tasks/index/project.application-services.v2.branch.main.latest.taskgraph/decision-nightly) from the task index.
   - Click the "View Task" link
   - Click "Task Group" in the top-left
   - You should now see a list of tasks from the latest nightly
     - `*-build` were for building the application.  A failure here indicates there's probably a breaking change that
       needs to be resolved.
     - To get the APK, navigate to `branch-build-fenix-build` and download `app-x86-debug.apk` from the artifacts list
     - `branch-build-ac-test.*` are the android-components tests tasks.  These are split up by gradle project, which matches
       how the android-components CI handles things.  Running all the tests together often leads to failures.
     - `branch-build-fenix-test` is the Fenix tests.  These are not split up per-project.
- These builds are triggered by our [.cron.yml](https://github.com/mozilla/application-services/blob/main/.cron.yml) file
