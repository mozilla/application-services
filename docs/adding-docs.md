# Developing documentation

The documentation in this repository pertains to the application-services library, primarily the sync and storage components, firefox account client and the nimbus-sdk experimentation client.

The markdown is converted to static HTML using [mdbook](https://rust-lang.github.io/mdBook/).  To add a new document, you need to add it to the SUMMARY.md file which produces the sidebar table of contents.

## Building documentation

### Building the narrative (book) documentation

The `mdbook` crate is required in order to build the documentation:

```sh
cargo install mdbook mdbook-mermaid mdbook-open-on-gh
```

The repository documents are be built with:

```sh
./tools/build.docs.sh
```

The built documentation is saved in `build/docs/book`.
