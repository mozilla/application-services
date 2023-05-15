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
  - Merge the `application-services` PR into `main` and manually trigger a nightly build.
  - On the next day, the application-services nightly bump PRs fail for the `firefox-android` and
    `firefox-ios` repositories since there are breaking changes.  This is expected and normal.
  - Merge your branches on `firefox-android` and `firefox-ios` into the branch with the nightly
    bump.  This should resolve the issues and the nightly can be merged as normal.
