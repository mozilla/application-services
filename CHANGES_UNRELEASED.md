**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v69.0.0...main)

## General

### What's Changed

- The bundled version of Glean has been updated to v34.0.0.
- The bundled version of the Nimbus SDK has been updated to v0.7.2.

## Autofill

### ⚠️ Breaking changes ⚠️

- The `NewCreditCardFields` record is now called `UpdatableCreditCardFields`.
- The `NewAddressFields` record is now called `UpdatableAddressFields`.

### What's Changed

- The `CreditCard` and `Address` records now exposes additional metadata around timestampes.
- The ability to sync incoming address records has been added.
