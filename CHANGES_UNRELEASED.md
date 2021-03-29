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

### ⚠️ Breaking changes ⚠️

## Autofill

* The credit-cards API has changed to support card numbers being encrypted.
  The card dictionary now has `cc_number_enc`, which is encrypted, and
  `cc_number_last_4` which is not. It is the responsibility of the embedding
  application to perform the crypto using new public functions available for
  this purpose, because in general, the component does not know the encryption
  key.

  The exception is when syncing, where the key is needed, so support has
  been added to allow the engine to know the key during a sync.

## Sync Manager

* The SyncParams struct has a new map named `local_encryption_keys` (or
  `localEncryptionKeys` in Kotlin) to support credit-card encryption. Due to
  limitations in the Kotlin support for protobufs, this new map must be
  specified - an emptyMap() is fine (although an entry will need to be
  specified once credit-card syncing is enabled.)

[Full Changelog](https://github.com/mozilla/application-services/compare/v74.0.1...main)
