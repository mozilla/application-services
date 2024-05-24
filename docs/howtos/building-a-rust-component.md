# Guide to Building a Syncable Rust Component

> This is a guide to creating a new Syncable Rust Component like many of the components in this repo. If you are looking for information how to build (ie,compile, etc) the existing components, you are looking for [our build documentation](../building.md)


Welcome!

It's great that you want to build a Rust Component - this guide should help
get you started. It documents some nomenclature, best-practices and other
tips and tricks to get you started.

This document is just for general guidance - every component will be different
and we are still learning how to make these components. Please update this
document with these learnings.

To repeat with emphasis - **please consider this a living document**.

# General design and structure of the component

We think components should be structured as described here.

## We build libraries, not frameworks

Think of building a "library", not a "framework" - the application should be in
control and calling functions exposed by your component, not providing functions
for your component to call.

## The "store" is the "entry-point"

[Note that some of the older components use the term "store" differently; we
should rename them! In Places, it's called an "API"; in Logins an "engine". See
`webext-storage` for a more recent component that uses the term "Store" as we
think it should be used.]

The "Store" is the entry-point for the consuming application - it provides the
core functionality exposed by the component and manages your databases and other
singletons. The responsibilities of the "Store" will include things like creating the
DB if it doesn't exist, doing schema upgrades etc.

The functionality exposed by the "Store" will depend on the complexity of the
API being exposed. For example, for `webext-storage`, where there are only a
handful of simple public functions, it just directly exposes all the
functionality of the component. However, for Places, which has a much more
complex API, the (logical) Store instead supplies "Connection" instances which
expose the actual functionality.

## Using sqlite

We prefer sqlite instead of (say) JSON files or RKV.

Always put sqlite into WAL mode, then have exactly 1 writer connection and as
many reader connections you need - which will depend on your use-case - for
example, `webext_storage` has 1 reader, while `places` has many.

(Note that places has 2 writers (one for sync, one for the api), but we
believe this was a mistake and should have been able to make things work
better with exactly 1 shared between sync and the api)

We typically have a "DB" abstraction which manages the database itself - the
logic for handling schema upgrades etc and enforcing the "only 1 writer" rule
is done by this.

However, this is just a convenience - the DB abstractions aren't really passed
around - we just pass raw connections (or transactions) around. For example, if
there's a utility function that reads from the DB, it will just have a Rusqlite
connection passed. (Again, older components don't really do this well, but
`webext-storage` does)

We try and leverage rust to ensure transactions are enforced at the correct
boundaries - for example, functions which write data but which must be done as
part of a transaction will accept a Rusqlite `Transaction` reference as the
param, whereas something that only reads the Db will accept a Rusqlite
`Connection` - note that because `Transaction` supports
`Deref<Target = Connection>`, you can pass a `&Transaction` wherever a
`&Connection` is needed - but not vice-versa.

### Meta-data

You are likely to have a table just for key/value metadata, and this table will
be used by sync (and possibly other parts of the component) to track the
sync IDs, lastModified timestamps etc.

### Schema management

The schemas are stored in the tree in .sql files and pulled into the source at
build time via `include_str!`. Depending on the complexity of your component,
there may be a need for different Connections to have different Sql (for
example, it may be that only your 'write' connection requires the sql to define
triggers or temp tables, so these might be in their own file.)

Because requirements evolve, there will be a need to support schema upgrades.
This is done by way of sqlite's `PRAGMA user_version` - which can be thought of
as simple metadata for the database itself. In short, immediately after opening
the database for the first time, we check this version and if it's less than
expected we perform the schema upgrades necessary, then re-write the version
to the new version.

This is easier to read than explain, so read the `upgrade()` function in
[the Places schema code](https://github.com/mozilla/application-services/blob/main/components/places/src/db/schema.rs)

You will need to be a big careful here because schema upgrades are going to
block the calling application immediately after they upgrade to a new version,
so if your schema change requires a table scan of a massive table, you are going
to have a bad time. Apart from that though, you are largely free to do whatever
sqlite lets you do!

Note that most of our components have very similar schema and database
management code - these are screaming out to be refactored so common logic can
be shared. Please be brave and have a go at this!

### Triggers

We tend to like triggers for encompassing application logic - for example, if
updating one row means a row in a different table should be updated based on
that data, we'd tend to prefer an, eg,  `AFTER UPDATE` trigger than having our
code manually implement the logic.

However, you should take care here, because functionality based on triggers is
difficult to debug (eg, logging in a trigger is difficult) and the functionality
can be difficult to locate (eg, users unfamiliar with the component may wonder
why they can't find certain functionity in the rust code and may not consider
looking in the sqlite triggers)

You should also be careful when creating triggers on persistent main tables.
For example, bumping the change counter isn't a good use for a trigger,
because it'll run for all changes on the table—including those made by Sync.
This means Sync will end up tracking its own changes, and getting into infinite
syncing loops. Triggers on temporary tables, or ones that are used for
bookkeeping where the caller doesn't matter, like bumping the foreign
reference count for a URL, are generally okay.

## General structure of the rust code

We prefer flatter module hierarchies where possible. For example, in `Places`
we ended up with `sync_history` and `sync_bookmarks` sub-modules rather than
a `sync` submodule itself with `history` and `bookmarks`.

Note that the raw connections are never exposed to consumers - for example, they
will tend to be stored as private fields in, eg, a Mutex.

# Syncing

The traits you need to implement to sync aren't directly covered here.

All meta-data related to sync must be stored in the same database as the
data itself - often in a `meta` table.

All logic for knowing which records need to be sync must be part of the
application logic, and will often be implemented using `triggers`. It's quite
common for components to use a "change counter" strategy, which can be
summarized as:

* Every table which defines the "top level" items being synced will have a
  column called something like 'sync_change_counter' - the app will probably
  track this counter manually instead of using a trigger, because sync itself
  will need different behavior when it updates the records.

* At sync time, items with a non-zero change counter are candidates for syncing.

* As the sync starts, for each item, the current value of the change counter is
  remembered. At the end of the sync, the counter is decremented by this value.
  Thus, items which were changed between the time the sync started and completed
  will be left with a non-zero change counter at the end of the sync.

## Syncing FAQs

This section is stolen from [this document](https://docs.google.com/document/d/1s9ld2F4e83eQ944kN6QXXTRlqrX74w2AJS6W2fDyAJ8)

### What’s the global sync ID and the collection sync ID?
Both guids, both used to identify when the data in the server has changed
radically underneath us (eg, when looking at lastModified is no longer a sane
thing to do.)

The "global sync ID" changing means that every collection needs to be assumed as
having changed radically, whereas just the "collection sync ID" changing means
just that one collection.

These global IDs are most likely to change on a node reassignment (which should
be rare now with durable storage), a password reset, etc. An example of when the
collection ID will change is a "bookmarks restore" - handling an old version of
a database re-appearing is why we store these IDs in the database itself.

### What’s `get_sync_assoc`, why is it important? What is `StoreSyncAssociation`?
They are all used to track the guids above. It’s vitally important we know when
these guids change.

StoreSyncAssociation is a simple enum which reflects the state a sync engine
can be in - either `Disconnected` (ie, we have no idea what the GUIDs are) or
`Connected` where we know what we think the IDs are (but the server may or may
not match with this)

These GUIDs will typically be stored in the DB in the metadata table.

### what is `apply_incoming` versus `sync_finished`
`apply_incoming` is where any records incoming from the server (ie, possibly
all records on the server if this is a first-sync, records with a timestamp
later than our last sync otherwise) are processed.

`sync_finished` is where we've done all the sync work other than uploading new
changes to the server.

### What's the diff between reset and wipe?

* Reset means “I don’t know what’s on the server - I need to reconcile everything there with everything I have”. IOW, a “first sync”
* Wipe means literally “wipe all server data”

# Exposing to consumers

You will need an FFI or some other way of exposing stuff to your consumers.

We use a tool called [UniFFI](https://github.com/mozilla/uniffi-rs/) to automatically
generate FFI bindings from the Rust code.

If UniFFI doesn't work for you, then you'll need to hand-write the FFI layer.
Here are some earlier blog posts on the topic which might be helpful:

* [Building and Deploying a Rust library on Android](https://mozilla.github.io/firefox-browser-architecture/experiments/2017-09-21-rust-on-android.html)
* [Building and Deploying a Rust library on iOS](https://mozilla.github.io/firefox-browser-architecture/experiments/2017-09-06-rust-on-ios.html)
* [Blog post re: lessons in binding to Rust code from iOS](https://discourse.mozilla.org/t/dear-diary-turns-out-x-platform-is-hard/25348)

The above are likely to be superseded by uniffi docs, but for now, good luck!
