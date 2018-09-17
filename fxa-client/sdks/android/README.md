# FxA Android SDK

The SDK lives in the [mozilla-mobile/android-components](https://github.com/mozilla-mobile/android-components/tree/master/components/service/firefox-accounts) repository.

## Rust component development

Run `cargo build -p fxa-client` to build the Rust component for your
development device and `cargo test -p fxa-client` to run the tests
locally.

Run `./gradlew :fxa-client-library:assemble{Debug,Release}` to build
the Rust component and Android AAR wrapper library for supported
target architectures.
