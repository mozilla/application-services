# Merino

A cross-platform Rust client library for Mozilla's [Merino](https://merino.services.mozilla.com) curated recommendations service. This powers the curated content recommendations (articles/stories) shown on Firefox's New Tab page.

## Overview

The library provides a `CuratedRecommendationsClient` that fetches curated recommendations from the Merino backend via its REST API (`/api/v1/curated-recommendations`). It uses [UniFFI](https://mozilla.github.io/uniffi-rs/) to generate cross-platform bindings for Android (and other targets).

## Features

- **Locale support** — 14 supported locales across English, French, Spanish, Italian, and German variants.
- **Content filtering** — Filter recommendations by region, topic, and section follow/block preferences.
- **Structured feeds** — Responses can include categorized sections (business, sports, tech, etc.), and an interest picker with responsive layout configurations.
- **A/B experiment support** — Pass experiment name and branch parameters to support server-side experimentation.
- **Cross-platform** — Rust core with UniFFI-generated bindings for Android (and other platforms).

## Architecture

- **`CuratedRecommendationsClient`** — Main entry point constructed with a base host and user agent header.
- **`models/`** — Request/response data types annotated with UniFFI and serde for serialization, split by domain:
  - `locale.rs` — Supported locale enum and parsing helpers, defined via a `define_locales!` macro to keep the variant list in one place.
  - `request.rs` — Client configuration, section settings, and request parameters.
  - `response.rs` — Response envelope, recommendation items, and interest picker types.
  - `feeds.rs` — Categorized feed containers, feed sections, and Fakespot product types.
  - `layout.rs` — Responsive layout, column, and tile configuration types.
- **`http.rs`** — HTTP layer built on Mozilla's `viaduct` library, with a trait-based design to allow injecting fake clients for testing.
- **`error.rs`** — Error types categorized as Network, Validation (422), BadRequest (400), Server (5xx), and Unexpected, with error reporting hooks via `error-support`.

## Testing

To run unit tests:

```sh
cargo test -p merino
```

The HTTP layer uses a trait (`HttpClientTrait`) so tests can inject fake clients to simulate success and error responses without making real network requests.

To test requests locally, run `cargo run --bin merino-cli -- --user-agent "my-cli/1.0" query --json '{ "locale": "en", "region": "US", "count": 4, "topics": ["tech"], "feeds": ["sections"] }'`, to use the cli implementation in the `application-services/examples/merino-cli` folder.
