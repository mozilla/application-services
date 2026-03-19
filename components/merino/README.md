# Merino

A cross-platform Rust client for Mozilla's [Merino](https://merino.services.mozilla.com) service. This library provides a `CuratedRecommendationsClient` that fetches a subset of information from the Merino backend via its REST API (`/api/v1/curated-recommendations`). It uses [UniFFI](https://mozilla.github.io/uniffi-rs/) to generate cross-platform bindings that those platforms will consume.

## Testing

To run unit tests:

```sh
cargo test -p merino
```

The HTTP layer uses a trait (`HttpClientTrait`) so tests can inject fake clients to simulate success and error responses without making real network requests.

To test requests locally, run `cargo run --bin merino-cli -- --user-agent "my-cli/1.0" query --json '{ "locale": "en", "region": "US", "count": 4, "topics": ["tech"], "feeds": ["sections"] }'`, to use the cli implementation in the `application-services/examples/merino-cli` folder.
