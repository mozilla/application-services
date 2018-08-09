## Firefox Application Services

### Contents

* [fxa-rust-client](fxa-rust-client) - cross compiled FxA Rust client that can work with Firefox Sync keys and more 
* [sandvich](sandvich) - Example apps that use SDKs built on top of `fxa-rust-client` to demonstrate an FxA login flow.
* [sync15-adapter](sync15-adapter) - Sync 1.5 adapter
* [libs](libs) - libs directory has build scripts for native libraries
* [docs](docs) - documentation sources 
* [website](website) - website built from documentation sources


### Other Resources

* [fxa-client-ios](https://github.com/eoger/fxa-client-ios) - an iOS framework that exposes `fxa-rust-client`.
* [mentat](https://github.com/mozilla/mentat) - a persistent, relational store inspired by Datomic and DataScript.
* [sync-server](https://github.com/mozilla-services/syncserver) - an all-in-one package for running a self-hosted Firefox Sync server.

### Tools

* [eqrion/cbindgen](https://github.com/eqrion/cbindgen) - generate C bindings from Rust code
* [rust-lang-nursery/rust-bindgen](https://github.com/rust-lang-nursery/rust-bindgen) - generate Rust FFI bindings to C (and some C++) libraries
