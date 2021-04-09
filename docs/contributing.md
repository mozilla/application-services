# Contributing to Application Services

Anyone is welcome to help with the Application Services project. Feel free to get in touch with other community members on Matrix, the mailing list or through issues here on GitHub.

- Matrix: [#sync:mozilla.org](https://chat.mozilla.org/#/room/#sync:mozilla.org)
- Mailing list: <https://mail.mozilla.org/listinfo/sync-dev>
- and of course, [the issues list](https://github.com/mozilla/application-services/issues)

Participation in this project is governed by the
[Mozilla Community Participation Guidelines](https://www.mozilla.org/en-US/about/governance/policies/participation/).

## Bug Reports ##

You can file issues here on GitHub. Please try to include as much information as you can and under what conditions
you saw the issue.

## Building the project ##

Build instructions are available [here](building.md). Do not hesitate to let us know which pain-points you had with setting up your environment!

## Finding issues ##

Below are a few different queries you can use to find appropriate issues to work on.  Feel free to reach out if you need any additional clarification before picking up an issue.

- **[good first issues](https://github.com/mozilla/application-services/issues?q=is%3Aopen+is%3Aissue+label%3Agood-first-issue)** -  If you are a new contributor, search for issues labeled `good-first-issue`
- **[good second issues](https://github.com/mozilla/application-services/labels/good-second-issue)** Once you've got that first PR approved and you are looking for something a little more challenging, we are keeping a list of next-level issues. Search for the `good-second-issue` label.
- **[papercuts](https://github.com/mozilla/application-services/issues?utf8=%E2%9C%93&q=is%3Aissue+is%3Aopen+%22Epic%3A+papercuts%22+)** A collection of smaller sized issues that may be a bit more advanced than a first or second issue.
- **[important, but not urgent](https://github.com/mozilla/application-services/issues?utf8=%E2%9C%93&q=is%3Aissue+is%3Aopen+%22Epic%3A+important+not+urgent%22)** - For more advanced contributors, we have a collection of issues that we consider important and would like to resolve sooner, but work isn't currently prioritized by the core team.


## Sending Pull Requests ##

Patches should be submitted as [pull requests](https://help.github.com/articles/about-pull-requests/) (PRs).

Before submitting a PR:
- Your patch should include new tests that cover your changes, or be accompanied by explanation for why it doesn't need any. It is your and your reviewer's responsibility to ensure your patch includes adequate tests.
  - Consult the [testing guide](./howtos/testing-a-rust-component.md) for some tips on writing effective tests.
- Your code should pass all the automated tests before you submit your PR for review.
  - The simplest way to confirm this is to run `cargo all_tests`, which uses the `./automation/all_tests.sh` script, that runs all test suites and linters for Rust, Kotlin and Swift code.
    - **Note:** You might choose to avoid running all automated tests to incrementally validate your changes as it can be a long-running process. Instead run a reasonable subset of all tests that will exercise your changes and then run `cargo all_tests` to validate your entire patch before submitting your PR.
  - "Work in progress" pull requests are welcome, but should be clearly labeled as such and should not be merged until all tests pass and the code has been reviewed.
    - You can label pull requests as "Work in progress" by using the Github PR UI to indicate this PR is a draft ([learn more about draft PRs](https://docs.github.com/en/github/collaborating-with-issues-and-pull-requests/about-pull-requests#draft-pull-requests)).
- Run `cargo fmt` to ensure your Rust code is correctly formatted. You should run this command after running tests and before pushing changes so that any fixes for failed tests are included.
  - If you have modified any Swift code, also run `swiftformat --swiftversion 4` on the modified code.
- Your patch should include a changelog entry in [CHANGES_UNRELEASED.md](../CHANGES_UNRELEASED.md) or an explanation of why
  it does not need one. Any breaking changes to Swift or Kotlin binding APIs should be noted explicitly
- If your patch adds new dependencies, they must follow our [dependency management guidelines](./dependency-management.md).
  Please include a summary of the due dilligence applied in selecting new dependencies.

When submitting a PR:
- You agree to license your code under the project's open source license ([MPL 2.0](/LICENSE)).
- Base your branch off the current `main` branch.
- Add both your code and new tests if relevant.
- Please do not include merge commits in pull requests; include only commits with the new relevant code.
- We encourage you to [GPG sign your commits](https://help.github.com/articles/managing-commit-signature-verification).

## Code Review ##

This project is production Mozilla code and subject to our [engineering practices and quality standards](https://developer.mozilla.org/en-US/docs/Mozilla/Developer_guide/Committing_Rules_and_Responsibilities). Every patch must be peer reviewed by a member of the Application Services team.
