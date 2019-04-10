# Introduction

The Application Services libraries provide cross-platform components for storing and syncing user data within the Firefox ecosystem. As we make Firefox available on more platforms, and ship more products that aren't web browsers, we're finding that each product wants to access existing Firefox data, like bookmarks, history, and logins, and make its own data available to others. The goal of the sync and storage components is to expose a uniform, flexible, and high-level way to do this.

* High-level means your application thinks in terms of _what_ it wants to do—"add a bookmark", "store a page visit", "update a saved password", "sync now"—and not _how_. The component takes care of details like defining the database schema, handling migrations, optimizing queries, marshaling and unmarshaling Sync records, and resolving merge conflicts.
* Uniform means one way to access data everywhere. The same building blocks are used in logins, bookmarks, and history, as well as any custom data types. These are also the same across products and platforms, so you don't need to change three different codebases.
* Flexible means it's easy for your application to add new data types, evolve its storage schema, and experiment with new ways to represent data.

## Why?

Historically, each product had to build its own storage and sync system. They often started with similar data models, but then evolved based on immediate product needs. Changes had to be backported to each platform, often across languages: Firefox Desktop was written in a mix of JavaScript and C++, Firefox for Android in Java, and Firefox for iOS in Swift.

Beyond the language barrier, there was little commonality between the implementations. Some concepts didn't translate well, if at all, and coordinating changes across platforms was hard. Syncing was often bolted on, and required lots of low-level integration work. This made for an inconsistent, brittle developer experience that affected the final product.

## Design

All components share a similar architecture:

* The **database** persists user data to disk. We currently use [SQLite](https://sqlite.org/) for all Rust components. It's possible to build a component around another store, like [rkv](https://crates.io/crates/rkv), but the existing data types are relational, so SQLite is a better fit. The database schema has tables for storing local data, as well as staging (or "mirror") tables for incoming and outgoing synced items. The staging tables also help with conflict resolution.

* The **storage layer** defines the domain objects—logins, bookmark items, pages, visits—and operations on them. This lives in the `components/{component}/src` directory. These CRUD-style operations, along with a sync  are exposed on a _syncable store interface_. The storage layer also manages database connections, and handles bookkeeping for Sync state. Other Rust crates can use the storage layer directly, without going through the FFI. This gives them an idiomatic API, type checking, and memory safety. However, we need two additional layers to use the component from a mobile app.

* The **FFI layer** is the glue between the application and storage. This lives in `components/{component}/ffi`. It's also written in Rust, and exposes `extern "C"` functions for the application layer. These functions are unsafe by necessity, as the FFI supports only a limited set of types. They also take care of details like managing calls from multiple threads, handing out safe references to Rust structures, and serializing and deserializing arguments using Protobufs.

* The **application layer** is an idiomatic binding for the application consuming the component. It's written in Kotlin for Android, and Swift for iOS. These live in `components/{component}/ios` and `components/{component}/android`.

Each component can either be consumed individually, or as part of a package called a _megazord_. Megazording centralizes global state management, and ensures that build artifacts only contain one copy of each library.

## The components

The components we currently provide are:

* Logins, for saved usernames and passwords. This component is used in [Lockbox](https://mozilla-lockbox.github.io/).
* Places, for bookmarks and history. This component is used in the [Android Reference Browser](https://github.com/mozilla-mobile/reference-browser/) and [Fenix](https://github.com/mozilla-mobile/fenix/).

We'll take a look at those components next!
