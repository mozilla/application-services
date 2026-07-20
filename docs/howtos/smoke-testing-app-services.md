# Smoke testing Application Services against end-user apps

This is a great way of finding integration bugs with `application-services`.
The testing can be done manually using substitution scripts, but we also have scripts that will do the smoke-testing for you.

## Dependencies

Run `pip3 install -r automation/requirements.txt` to install the required Python packages.


## Usage

You can easily run a smoke test against iOS and Fenix by running the following: 

`./automation/build_against_all.py --firefox-dir ../firefox --action build-without-testing --allow-clears`

In this case, `firefox-dir` must point to a bootstrapped and working installation of `mozilla-central` (see instructions [here](https://firefox-source-docs.mozilla.org/contributing/contribution_quickref.html)). It is used for the compilation and test of the Android build.

You can also run against specific platforms with the following examples:

- iOS: 
    
    ```./automation/build_against_ios.py  --action build-without-testing --clear-previous-bindings --clean-ios-caches --use-local-repo ../firefox-ios```

    - By default, this script creates a `tmp` directory for `firefox-ios` and uses it. If you have a running `firefox-ios`, you can add the argument `--use-local-repo ../firefox-ios` (as shown), which may result in speed gains and the ability to run it on XCode immediately after a failure.

    - You can also customize the run scheme (`--scheme`) or test plan (`--test-plan`). Available schemes include `Fennec` (default) and `Firefox`. Available test plans include: `Smoketest`, `FullFunctionalTestPlan` and `UnitTest`.

        - A full list of schemes and their corresponding test plans can be found [in the firefox-ios](https://github.com/mozilla-mobile/firefox-ios/tree/main/firefox-ios/Client.xcodeproj/xcshareddata/xcschemes) respository.

- Fenix: 

    ```./automation/build_against_fenix.py --action build-without-testing --firefox-dir ../firefox  --prefix-ff fenix --clear-previous-bindings```

    - The `--prefix-ff` argument here refers to the prefix passed to commands like `./gradlew fenix:assembleDebug`. It can be omitted, but may cause failures on non-fenix projects.

    - The `--clear-previous-bindings` argument here runs a `./gradlew prefix:clear` before recompiling. It is not always necessary, and can be excluded for some speed gains, but can result in some cache reuse.

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
