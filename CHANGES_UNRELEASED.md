**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes
 
 ## iOS

- Addition of the `Nimbus` helper object for interacting with the Nimbus SDK; this introduces some ergonomics around threading and error reporting.

## Autofill

### ⚠️ Breaking changes ⚠️

* The credit-cards API has changed to support card numbers being encrypted.
  The card dictionary now has `cc_number_enc`, which is encrypted, and
  `cc_number_last_4` which is not. It is the responsibility of the embedding
  application to perform the crypto using new public functions available for
  this purpose, because in general, the component does not know the encryption
  key.

  The exception is when syncing, where the key is needed, so support has
  been added to allow the engine to know the key during a sync.

[Full Changelog](https://github.com/mozilla/application-services/compare/v74.0.1...main)
