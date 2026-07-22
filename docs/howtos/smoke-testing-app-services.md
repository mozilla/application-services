# Smoke testing Application Services against end-user apps

This is a great way of finding integration bugs with `application-services`.
The testing can be done manually using substitution scripts, but we also have scripts that will do the smoke-testing for you.

## Dependencies

Run `pip3 install -r automation/requirements.txt` to install the required Python packages.


## Usage

You can easily run a smoke test against iOS and Fenix by running the following: 

`./automation/build_against_all.py --firefox-dir ../firefox --action build-without-testing --allow-clears`

- In this case, `firefox-dir` must point to a bootstrapped and working installation of `mozilla-central` (see instructions [here](https://firefox-source-docs.mozilla.org/contributing/contribution_quickref.html)). It is used for the compilation and test of the Android and HNT builds.

- The `--action` argument can be either `build-without-testing` or `run-tests`. 

- The `--allow-clears` argument allows the various subscripts to clear their various caches as appropriate (such as XCode cleaning iOS caches).

You can also run against specific platforms directly with the following examples:

- iOS: 
    
    ```./automation/build_against_ios.py  --action build-without-testing --clear-previous-bindings --clean-ios-caches --use-local-firefox-ios ../firefox-ios```

    - By default, this script creates a `tmp` directory for `firefox-ios` and uses it. If you have a running `firefox-ios`, you can add the optional argument `--use-local-firefox-ios ../firefox-ios` (as shown), which may result in speed gains and the ability to run it on XCode immediately after a failure. This can also be passed to `./build_against_all.py`.

    - You can also customize the run scheme (`--scheme`) or test plan (`--test-plan`). Available schemes include `Fennec` (default) and `Firefox`. Available test plans include: `Smoketest`, `FullFunctionalTestPlan` and `UnitTest`.

        - A full list of schemes and their corresponding test plans can be found [in the firefox-ios](https://github.com/mozilla-mobile/firefox-ios/tree/main/firefox-ios/Client.xcodeproj/xcshareddata/xcschemes) respository.

        - These can be used in `./build_against_all.py` as `--ios-scheme` and `--ios-test-plan` respectively.

- Fenix: 

    ```./automation/build_against_fenix.py --action build-without-testing --firefox-dir ../firefox  --prefix-ff fenix --clear-previous-bindings```

    - The `--prefix-ff` argument here refers to the prefix passed to commands like `./gradlew fenix:assembleDebug`. It can be omitted, but may cause failures on non-fenix projects.

    - The `--clear-previous-bindings` argument here runs a `./gradlew prefix:clear` before recompiling. It is not always necessary, and can be excluded for some speed gains, but can result in some cache reuse.

- HNT:

    ``` ./automation/build_against_hnt.py --action build-without-testing --firefox-dir ../firefox```

    - This script uses the symlink method described [here](./locally-published-components-in-firefox-hnt.md), cleaning up after a successful run. You can avoid this cleanup process (specifically on a successful run) by passing `--no-clean-up`, which will keep the symlinks. For example, you might run with `--action build-without-testing --no-clean-up` to experiment after with `./mach run`.

    - Unlike the other tests, HNT has the additional `action` variant of `--action run`, because it can be run from the terminal directly. 


All test scripts also accept the `--verbose` argument to show the output of run subprocesses (such as `./mach build`).


## Limitations

Note that these tests are primarily smoke tests against the building and compilation of application-services. There are a wide array of possible regressions that can only be caught with tests, including ones that crash the build immediately on running. To ensure any regressions for your component are caught, tests should be created for them rather than just building. 

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
