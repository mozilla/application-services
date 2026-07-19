# Smoke testing Application Services against end-user apps

This is a great way of finding integration bugs with `application-services`.
The testing can be done manually using substitution scripts, but we also have scripts that will do the smoke-testing for you.

## Dependencies

Run `pip3 install -r automation/requirements.txt` to install the required Python packages.


## Usage

You can easily run a smoke test against iOS and Fenix by running the following: 

`./automation/build_against_all.py --firefox-dir ../firefox --action build-without-testing --allow-clears`

In this case, `firefox-dir` must point to a bootstrapped and working installation of `mozilla-central` (see instructions [here](https://firefox-source-docs.mozilla.org/contributing/contribution_quickref.html)). It is used for the compilation and test of the Android build.

You can also run against specific platforms the following way:

- iOS: 
    
    ```./automation/build_against_ios.py --clear-previous-bindings --clean-ios-caches --action build-without-testing```

- Fenix: 

    ```./automation/build_against_fenix.py --action build-without-testing --firefox-dir ../firefox```

All test scripts also accept the `--verbose` argument to show the output of run subprocesses (such as `./mach build`).

## Deprecated tests

### Android Components

The `automation/smoke-test-android-components.py` script will clone (or use a local version) of
android-components and run a subset of its tests against the current `application-services` worktree.
It tries to only run tests that might be relevant to `application-services` functionality.

### Fenix

The `automation/smoke-test-fenix.py` script will clone (or use a local version) of Fenix and
run tests against the current `application-services` worktree.

### Firefox iOS
The `automation/smoke-test-fxios.py` script will clone (or use a local version) of Firefox iOS and
run tests against the current `application-services` worktree.
