# Breaking changes in application-services code

Application-services components are consumed by multiple consumers including Firefox Android,
Firefox iOS, Focus Android, and Focus iOS.  To minimize the disruption to those projects when making
breaking API changes, we follow a simple rule: **Have approved PRs ready to land that fix the
breakage in the other repos before merging the PR into application-services**.

This means writing code for the
[firefox-android](https://github.com/mozilla-mobile/firefox-android/) and
[firefox-ios](https://github.com/mozilla-mobile/firefox-ios/) repositories that resolves any
breaking changes, creating a PR in those repositories, and waiting for it to be approved.

You can test this code locally using the autopublish flow ([Android](./locally-published-components-in-fenix.md), [iOS](./locally-published-components-in-firefox-ios.md)) and use the [branch build system](./branch-builds.md) to run CI tests.

## Merging

Do not merge any PRs until all are approved.  Once they are all approved then:
  - Merge the `application-services` PR into `main`
  - Manually trigger a new nightly build using the taskcluster hook:
    https://firefox-ci-tc.services.mozilla.com/hooks/project-releng/cron-task-mozilla-application-services%2Fnightly
  - Once the nightly task completes, trigger a new rust-components-swift build using the github action:
    https://github.com/mozilla/rust-components-swift/actions/workflows/update-as-nightly.yml
  - Update the `firefox-android` and `firefox-ios` PRs to use the newly built nightly:
    * [example of firefox-android changes](https://github.com/mozilla-mobile/firefox-android/pull/4056/files)
    * [example of firefox-ios changes](https://github.com/mozilla-mobile/firefox-ios/pull/16783/files)
  - Ideally, get the PRs merged before the firefox-android/firefox-ios nightly bump the next day.
    If you don't get these merged, then the nightly bump PR will fail.  Add a link to your PR in
    the nightly bump PR so the mobile teams know how to fix this.
