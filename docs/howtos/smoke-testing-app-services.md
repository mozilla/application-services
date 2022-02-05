# Smoke testing Application Services against end-user apps

This is a great way of finding integration bugs with `application-services`.
The testing can be done manually using substitution scripts, but we also have scripts that will do the smoke-testing for you.

## Android Components

The `automation/smoke-test-android-components.py` script will clone (or use a local version) of
android-components and run a subset of its tests against the current `application-services` worktree.
It tries to only run tests that might be relevant to `application-services` functionality.

## Fenix

The `automation/smoke-test-fenix.py` script will clone (or use a local version) of Fenix and
run tests against the current `application-services` worktree.

## ⚠️ Firefox iOS ⚠️
We don't currently have an automated way to smoke test against Firefox iOS. For now, you will have to setup a local build of Firefox iOS and have it build against a local build of application services. This process is not straightforward, and unfortunately not documented well. For a similar example based on Focus iOS, please check the doc on [how to test locally against Focus iOS](./locally-published-spm-in-ios.md)

- Bug on file to write a document for locally testing against Firefox iOS: [#4825](https://github.com/mozilla/application-services/issues/4825)
- Bug on file for creating an automation script to smoke test against Firefox iOS: [#4826](https://github.com/mozilla/application-services/issues/4826)
