This crate is an example Rust component.  It aims to show how components are generally structured
and the best practices to use when creating one.

For this example, we're going to build the classic TODO app example.  Our app will feature:

* UniFFI-generated Kotlin/Swift bindings
* SQLite persistence
* Error reporting, that would report to Sentry if we used this in firefox-android
* An HTTP REST client.

Each file focuses on a particular part of the component:

* `lib.rs`: Defining a public API and using UniFFI to expose it
* `error.rs`: Error hierarchies and error reporting
* `schema.rs`: SQLite schema migrations
* `db.rs`: SQLite DB operations
* `http.rs`: HTTP requests
* `../Cargo.toml`: Dependency management
* `../../../examples/example-cli`: Creating a CLI for your component

Feel free to ignore files that don't relate to your component and to copy+paste the ones that do.
