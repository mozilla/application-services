# Contributing to Application Services

Anyone is welcome to help with the Application Services project. Feel free to get in touch with other community members on IRC, the
mailing list or through issues here on GitHub.

- IRC: `#sync` on `irc.mozilla.org`
- Mailing list: <https://mail.mozilla.org/listinfo/sync-dev>
- and of course, [the issues list](https://github.com/mozilla/application-services/issues)

Participation in this project is governed by the
[Mozilla Community Participation Guidelines](https://www.mozilla.org/en-US/about/governance/policies/participation/).

## Bug Reports ##

You can file issues here on GitHub. Please try to include as much information as you can and under what conditions
you saw the issue.

## Making Code Changes ##

To work on the code in this repo you will need to be familiar with
the [Rust](https://www.rust-lang.org/) programming language.
You can get a working rust compiler and toolchain via [rustup](https://rustup.rs/).

The Network Security Suite (NSS) libraries must be available on your system for
tests to run correctly.
You can install them by doing the following:

* On MacOS (using the [Homebrew package manager](https://brew.sh/)):
```
brew install nss
brew link --force nss
```
* On Debian/Ubuntu:
```
apt-get install libnss3-dev
```

Some components also require `openssl` and `sqlcipher` in order to build correctly.
You may be able to install these via your OS package manager, but for consistency
we recommend using the versions included in this repo by doing the following from
the root of your checkout:

```
source ./libs/bootstrap-desktop.sh
```

This will compile the dependencies if necessary and export environment variables
to configure your rust build to use them.

You can check that all dependencies are installed correctly by running the following from the
root of your checkout:

```
cargo test --all
```

If you plan to work on the Android component bindings, you should also review
the instructions for [setting up an Android build environment](https://github.com/mozilla/application-services/blob/master/docs/howtos/setup-android-build-environment.md)

## Sending Pull Requests ##

Patches should be submitted as [pull requests](https://help.github.com/articles/about-pull-requests/) (PRs).

Before submitting a PR:
- Your code must run and pass all the automated tests before you submit your PR for review. "Work in progress" pull requests are allowed to be submitted, but should be clearly labeled as such and should not be merged until all tests pass and the code has been reviewed.
  - Run `cargo test --all` to make sure all tests still pass and no warnings are emitted.
  - Run `cargo clippy --all-targets --all-features` to make sure that linting passes (You may need to `rustup component add clippy` first).
  - Run `cargo fmt` to ensure the code is formatted correctly.
- Your patch should include new tests that cover your changes. It is your and your reviewer's responsibility to ensure your patch includes adequate tests.

When submitting a PR:
- You agree to license your code under the project's open source license ([MPL 2.0](/LICENSE)).
- Base your branch off the current `master` (see below for an example workflow).
- Add both your code and new tests if relevant.
- Run `cargo test` and `cargo clippy` to make sure your code passes linting and tests.
- Please do not include merge commits in pull requests; include only commits with the new relevant code.

## Code Review ##

This project is production Mozilla code and subject to our [engineering practices and quality standards](https://developer.mozilla.org/en-US/docs/Mozilla/Developer_guide/Committing_Rules_and_Responsibilities). Every patch must be peer reviewed by a member of the Application Services team.
