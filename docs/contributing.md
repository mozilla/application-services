# Contributing to Application Services

Anyone is welcome to help with the [Application Services](index.md) project. Feel free to get in touch with [other community members on Matrix or through issues on GitHub.](./index.md#contact-us)

Participation in this project is governed by the
[Mozilla Community Participation Guidelines](https://www.mozilla.org/en-US/about/governance/policies/participation/).

## Bug Reports ##

You can file issues on [GitHub](https://github.com/mozilla/application-services/issues). Please try to include as much information as you can and under what conditions
you saw the issue.

## Building the project ##

Build instructions are available in the [`building`](building.md) page. Please let us know if you encounter any pain-points setting up your environment.

## Finding issues ##

Below are a few different queries you can use to find appropriate issues to work on. Feel free to reach out if you need any additional clarification before picking up an issue.

- **[good first issues](https://github.com/mozilla/application-services/issues?q=is%3Aopen+is%3Aissue+label%3Agood-first-issue)** -  If you are a new contributor, search for issues labeled `good-first-issue`
- **[good second issues](https://github.com/mozilla/application-services/labels/good-second-issue)** - Once you've got that first PR approved and you are looking for something a little more challenging, we are keeping a list of next-level issues. Search for the `good-second-issue` label.
- **[papercuts](https://github.com/mozilla/application-services/issues?utf8=%E2%9C%93&q=is%3Aissue+is%3Aopen+%22Epic%3A+papercuts%22+)** - A collection of smaller sized issues that may be a bit more advanced than a first or second issue.
- **[important, but not urgent](https://github.com/mozilla/application-services/issues?utf8=%E2%9C%93&q=is%3Aissue+is%3Aopen+%22Epic%3A+important+not+urgent%22)** - For more advanced contributors, we have a collection of issues that we consider important and would like to resolve sooner, but work isn't currently prioritized by the core team.


## Sending Pull Requests ##
Patches should be submitted as [pull requests](https://help.github.com/articles/about-pull-requests/) (PRs).

> When submitting PRs, We expect external contributors to push patches to a [fork](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks) of [`application-services`](https://github.com/mozilla/application-services). For more information about submitting PRs from forks, read [GitHub's guide](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-a-pull-request-from-a-fork).

Before submitting a PR:
- Your patch should include new tests that cover your changes, or be accompanied by explanation for why it doesn't need any. It is your and your reviewer's responsibility to ensure your patch includes adequate tests.
  - Consult the [testing guide](./howtos/testing-a-rust-component.md) for some tips on writing effective tests.
- Your code should pass all the automated tests before you submit your PR for review.
  - Before pushing your changes, run `./automation/tests.py changes`. The script will calculate which components were changed and run test suites, linters and formatters against those components. Because the script runs a limited set of tests, the script should execute in a fairly reasonable amount of time.
    - If you have modified any Swift code, also run `swiftformat --swiftversion 5` on the modified code.
- Your patch should include a changelog entry in [CHANGELOG.md](https://github.com/mozilla/application-services/blob/main/CHANGELOG.md) or an explanation of why
  it does not need one. Any breaking changes to Swift or Kotlin binding APIs should be noted explicitly.
- If your patch adds new dependencies, they must follow our [dependency management guidelines](./dependency-management.md).
  Please include a summary of the due diligence applied in selecting new dependencies.
- After you open a PR, our Continuous Integration system will run a full test suite.  It's possible that this step will result in errors not caught with the script so make sure to check the results.
- "Work in progress" pull requests are welcome, but should be clearly labeled as such and should not be merged until all tests pass and the code has been reviewed.
  - You can label pull requests as "Work in progress" by using the Github PR UI to indicate this PR is a draft ([learn more about draft PRs](https://docs.github.com/en/github/collaborating-with-issues-and-pull-requests/about-pull-requests#draft-pull-requests)).

When submitting a PR:
- You agree to license your code under the project's open source license ([MPL 2.0](https://github.com/mozilla/application-services/blob/main/LICENSE)).
- Base your branch off the current `main` branch.
- Add both your code and new tests if relevant.
- Please do not include merge commits in pull requests; include only commits with the new relevant code.
- We encourage you to [GPG sign your commits](https://help.github.com/articles/managing-commit-signature-verification).

## Code Review ##

This project is production Mozilla code and subject to our [engineering practices and quality standards](https://developer.mozilla.org/en-US/docs/Mozilla/Developer_guide/Committing_Rules_and_Responsibilities). Every patch must be peer reviewed by a member of the Application Services team.
