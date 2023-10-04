# Smoke testing Application Services against end-user apps

This is a great way of finding integration bugs with `application-services`.
The testing can be done manually using substitution scripts, but we also have scripts that will do the smoke-testing for you.

## Dependencies

Run `pip3 install -r automation/requirements.txt` to install the required Python packages.

## Android Components

The `automation/smoke-test-android-components.py` script will clone (or use a local version) of
android-components and run a subset of its tests against the current `application-services` worktree.
It tries to only run tests that might be relevant to `application-services` functionality.

## Fenix

The `automation/smoke-test-fenix.py` script will clone (or use a local version) of Fenix and
run tests against the current `application-services` worktree.

## Firefox iOS
The `automation/smoke-test-fxios.py` script will clone (or use a local version) of Firefox iOS and
run tests against the current `application-services` worktree.
