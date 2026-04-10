# Ads Client

## Overview

Mozilla Ads Client (also referred to as "MAC") is a library that handles integration with the Mozilla Ads Routing Service (MARS). It is primarily intended for use on mobile surfaces, namely Firefox iOS and Android, but can be used by any surface able to ingest components from Application Services.

The Ads Client component can request and display standard ad placements, and calls the appropriate callback URLs to send anonymized impressions and clicks back to Mozilla. MAC also provides a facility to report user dissatisfaction with ads so we can take appropriate action as necessary.

Like MARS, Ads Client is privacy-first. It does not track user information and it does not send sensitive identifiable information to Mozilla. Of the information Mozilla does receive, anything shared with advertisers is aggregated and/or de-identified to preserve user privacy.

While we welcome outside feedback and are committed to open source, this library is intended solely for use on Mozilla properties.

This component is currently still under construction.

## Tests

### Unit Tests

Unit tests are run with

```shell
cargo test -p ads-client
```

### Integration Tests

Integration tests make real HTTP calls to the Mozilla Ads Routing Service (MARS) staging environment. They live in a dedicated crate (`integration-tests/`) and are marked `#[ignore]` so they do not run with a plain `cargo test`.

They are run by the dedicated GitHub Actions workflow (`.github/workflows/ads-client-tests.yaml`), and can also be run manually:

```shell
cargo test -p ads-client-integration-tests -- --ignored
```

To run a specific test file or test:

```shell
cargo test -p ads-client-integration-tests --test mars -- --ignored
cargo test -p ads-client-integration-tests --test http_cache -- --ignored
cargo test -p ads-client-integration-tests --test mars test_contract_image_staging -- --ignored
```

**Note:** Integration tests require network access and will make real HTTP requests to the MARS staging API.

## Usage

Full API reference and usage guides for each supported language:

- [Swift](./docs/usage-swift.md)
- [Kotlin](./docs/usage-kotlin.md)
- [JavaScript](./docs/usage-javascript.md)

Each guide is a complete standalone document containing all type definitions, API tables, cache behavior documentation, and code examples in that language.
