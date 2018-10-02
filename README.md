## Firefox Application Services

### Contents

* [fxa-client](fxa-client) - cross compiled FxA Rust client that can work with Firefox Sync keys and more
* [sandvich](sandvich) - Example apps that use SDKs built on top of `fxa-client` to demonstrate a FxA login flow.
* [sync15-adapter](sync15-adapter) - Sync 1.5 adapter
* [libs](libs) - libs directory has build scripts for native libraries
* [docs](docs) - documentation sources 
* [website](website) - website built from documentation sources


### Overview

The following diagram describes how some of the components hosted here relate to each other within an app.

<img src="https://www.lucidchart.com/publicSegments/view/99d1529a-585a-4f43-bfe8-26ba82f3db51/image.png" width="500" />

### Other Resources

* [mentat](https://github.com/mozilla/mentat) - a persistent, relational store inspired by Datomic and DataScript.
* [sync-server](https://github.com/mozilla-services/syncserver) - an all-in-one package for running a self-hosted Firefox Sync server.

### Tools

* [eqrion/cbindgen](https://github.com/eqrion/cbindgen) - generate C bindings from Rust code
* [rust-lang-nursery/rust-bindgen](https://github.com/rust-lang-nursery/rust-bindgen) - generate Rust FFI bindings to C (and some C++) libraries
