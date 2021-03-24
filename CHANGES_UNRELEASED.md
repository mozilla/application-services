**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

 ## General

- The nimbus-sdk repo has been imported (with git history) into
  `components/nimbus-sdk`.  It is no longer a submodule.  Developers
  may need to execute

  ```bash
  rm -fR components/external/nimbus-sdk
  ```

  This is not expected to have any ramifications for consumers.
 ## iOS

- Addition of the `Nimbus` helper object for interacting with the Nimbus SDK; this introduces some ergonomics around threading and error reporting.

[Full Changelog](https://github.com/mozilla/application-services/compare/v74.0.1...main)
