# Places

Places is a library for storing and syncing bookmarks and history. It exposes high-level APIs for common use cases, like recording history visits, autocompleting visited URLs, clearing and expiring history, and managing bookmarks. It also provides engines for syncing visited pages and bookmarks through Firefox Sync.

In the past, all Mozilla browsers handled history and bookmark storage differently. Desktop used its [own implementation, written in a mix of C++ and JavaScript](https://developer.mozilla.org/en-US/docs/Mozilla/Tech/Places); Firefox for Android had [one written in Java](https://searchfox.org/mozilla-central/rev/f1c7ba91fad60bfea184006f3728dd6ac48c8e56/mobile/android/base/java/org/mozilla/gecko/db/BrowserDB.java); and Firefox for iOS had a [third implementation in Swift](https://github.com/mozilla-mobile/firefox-ios/blob/ceb3cc9f0cc1ad8a90ad465b8c74a855121f319d/Storage/SQL/BrowserSchema.swift). All platforms used [SQLite](https://sqlite.org/) for storage, with a similarly-shaped schema, but the similarities ended there. Each implementation evolved independently, dictated by immediate product needs, and concepts like [frecency](https://developer.mozilla.org/en-US/docs/Mozilla/Tech/Places/Frecency_algorithm) and address autocomplete heuristics weren't shared at all. Each platform also had its own sync implementation. The goal of Rust Places is to unify these features across all platforms.

Places is based on the Firefox Desktop implementation, and uses a mostly backward-compatible storage schema. It can either be consumed directly, as in Firefox for iOS, or through a layer like [Mozilla Android Components](https://mozac.org/).

## Architecture

`PlacesApi` is the entry point to Places. It's expected that your app or component will create one `PlacesApi` instance, and use it as a singleton. `PlacesApi` manages database connections and global Sync state. You can use `PlacesApi::open_connection` to open multiple read-only connections, or acquire the single read-write connection. Syncing for the first time also opens a special read-write connection for Sync, but this connection is not exposed to applications.

`open_connection` returns a `PlacesDb`. This is a wrapper around a [Rusqlite](https://docs.rs/rusqlite) `Connection` with shared `PRAGMA`s, SQL functions, and support for interrupting long-running queries. Under the hood, `PlacesDb` supports encrypted (via [SQLCipher](https://www.zetetic.net/sqlcipher/)) and unencrypted SQLite databases.

When you're done with a connection, make sure to close it using `PlacesApi::close_connection`.

## Connection lifecycle

Opening a read-write connection to an existing database runs migrations to bring the database up to date. The current schema and version are compiled in to the Rust Places library. Opening a database with a newer schema version is also supported, but the version will be rolled back to the current version. Opening a connection to a file that doesn't exist creates and initializes an empty database at that path.

In addition to the on-disk schema, a read-write connection also sets up temporary, in-memory tables and triggers. These are used internally for operations like deletion and frecency recalculation. The special Sync connection also defines its own tables and triggers to help with syncing. Read-only connections don't have these.

All `PlacesDb` connections use SQLite's [WAL](https://sqlite.org/wal.html) mode. This is faster than the default rollback journal, and allows concurrent reads and writes. However, this does mean that read-only connections might not immediately see changes from the read-write or Sync connections. Reading occasionally stale data is fine for cases like autocomplete matching, where performance is critical above everything else. For other cases, such as populating history and bookmarks views, it's better to always read using the read-write connection. That way, reads after writes always return the just-written data.

WAL mode doesn't allow multiple writers. This means that, for example, adding a bookmark or navigating to a page during a sync may cause one of the operations to fail with a "database locked" (`SQLITE_BUSY`) error. `PlacesDb` doesn't currently handle this.

Once you have a connection, you can fetch history pages and bookmarks from the database, and run autocomplete matches for typed URLs. If you have a read-write connection, you can also record new visits and organize bookmarks.

## API and storage layers

The Places library is split into three layers:

* An API layer that expresses higher-level concepts, like recording a history visit, adding, removing, or fetching a bookmark, syncing, or autocompleting a URL in the address bar.
* A storage layer that manages database connections and provides convenience methods for executing SQL statements.
* A [foreign function interface](https://docs.rs/ffi-support) (FFI) that wraps API and storage calls for Swift and Kotlin consumers.
