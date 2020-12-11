# Low-level sync-1.5 helper component

This component contains utility code to be shared between different
data stores that want to sync against a Firefox Sync v1.5 sync server.
It handles things like encrypting/decrypting records, obtaining and
using storage node auth tokens, and so-on.

There are 2 key concepts to understand here - the implementation itself, and
a rust trait for a "syncable store" where component-specific logic lives - but
before we dive into them, some preamble might help put things into context.

## We are bad at names!

Before we start, a quick note on names.

In Desktop Firefox, we have 2 main concepts for syncing - there's a "store"
(which is an interface to the underlying storage) and an "engine" (the thing
that interfaces between the "store" and the storage server to do the actual
syncing). Note that the "store" is an interface to the store, not the store
itself - eg, the "passwords" store exists in the "sync" directory and is an
interface to the actual password storage code that exists externally in the
browser. In other words, the sync functionality and how the data was actually
stored were completely separate.

The main problem with this is that the real store (ie, the passwords store in
the browser itself) was typically unaware of sync - sync was bolted on to the
side, which was undesirable in many ways (eg, knowing when an item was modified
and thus should be uploaded to the server was problematic)

For the rust components we avoided this mistake - the underlying store was not
only sync aware, but also took responsibility for syncing that store, as it
knew best how to manage the data. In other words, each component can be
considered a "syncable store".

However, we sadly chose to use the name "store" for the part of the component
that does the syncing. This is OK when only thinking about Sync, but once you
take a wider view, it becomes problematic - if you talk to (say) Android
Components about the "password store", they are likely to think about the
passwords DB and the API Android uses to get or save passwords, and not
specifically about the thing that *syncs* passwords.

In 2020, we bit the bullet and decided that new components should avoid this
confusion. Moving forward:

* The term "store" should be used as the interface to the database - ie, the
  thing that gets and saves items - the thing that an Android Developer using
  the component is likely to consider the "store". The newer webext-storage and
  autofill components have used this convention.

* We should reuse the term "engine" (or ideally, "sync engine") to talk about
  the thing that actually syncs - it's not a great name, but at least it's
  consistent with desktop.

* We should rename things that are named "Store" in the old components (ie,
  logins, places, tabs)

So in this document, I'm going to use the term "engine", even though some of
the code refers to the term "store".

## Introduction and History

For many years Sync has worked exclusively against a "sync v1.5 server". This
[is a REST API described here](https://mozilla-services.readthedocs.io/en/latest/storage/apis-1.5.html).
The important part is that the API is conceptually quite simple - there are
arbitrary "collections" containing "records" indexed by a GUID, and lacking
traditonal database concepts like joins. Because the record is encrypted,
there's very little scope for the server to be much smarter. Thus it's
reasonably easy to create a fairly generic abstraction over the API that can be
easily reused.

Back in the deep past, we found ourselves with 2 different components that
needed to sync against a sync v1.5 server. The apps using these components
didn't have schedulers or any UI for choosing what to sync - so these
components just looked at the existing state of the engines on the server and
synced if they were enabled.

This was also pre-megazord - the idea was that apps could choose from a "menu"
of components to include - so we didn't really want to bind these components
together. Therefore, there was no concept of "sync all" - instead, each of the
components had to be synced individually. So this component started out as more
of a "library" than a "component" which individual components could reuse - and
each of these components was a "syncable store" (ie, a store which could supply
 a "sync engine").

Fast forward to Fenix and we needed a UI for managing all the engines supported
there, and a single "sync now" experience etc - so we also have a sync_manager
component - [see its README for more](../components/sync_manager/README.md).
But even though it exists, there are still some parts of this component that
reflect these early days - for example, it's still possible to sync just a
single component using sync15 (ie, without going via the "sync manager"),
although this isn't used and should be removed - the "sync manager" allows you
to choose which engines to sync, so that should be used exclusively.

## Metadata

There's some metadata associated with a sync. Some of the metadata is "global"
to the app (eg, the enabled state of engines, information about what servers to
use, etc) and some is specific to an engine (eg, timestamp of the
server's collection for this engine, guids for the collections, etc).

We made the decision early on that no storage should be done by this
component:

* The "global" metadata should be stored by the application - but because it
  doesn't need to interpret the data, we do this with an opaque string (that
  is JSON, but the app should never assume or introspect that)

* Each engine should store its own metadata, so we don't end up in the
  situation where, say, a database is moved between profiles causing the
  metadata to refer to a completely different data set. So each engine
  stores its metadata in the same database as the data itself, so if the
  database is moved or copied, the metadata comes with it)

## Sync Implementation

The core implementation does all of the interaction with things like the
tokenserver, the `meta/global` and `info/collections` collections, etc. It
does all network interaction (ie, individual engines don't need to interact with
the network at all), tracks things like whether the server is asking us to
"backoff" due to operational concerns, manages encryption keys and the
encryption itself, etc. The general flow of a sync - which interacts with the
`Store` trait - is:

* Does all pre-sync setup, such as checking `meta/global`, and whether the
  sync IDs on the server match the sync IDs we last saw (ie, to check whether
  something drastic has happened since we last synced)
* Asks the store about how to formulate the URL query params to obtain the
  records the store cares about. In most cases, this will simply be "records
  since the last modified timestamp of the last sync".
* Downloads and decrypts these records.
* Passes these records to the store for processing, and obtains records that
  should be uploaded to the server.
* Encrypts these outgoing records and uploads them.
* Tells the store about the result of the upload (ie, the last-modified
  timestamp of the POST so it can be saved as store metadata)

As above, the sync15 component really only deals with a single store at a time.
See the "sync manager" for how multiple stores are managed (but the tl;dr is
that the "sync manager" leans on this very heavily, but knows about multiple
stores and manages shared state)

## The `Store` trait

The store trait is where all logic specific to a collection lives. A "sync
engine" implements this "Store" trait to implement actual syncing. As mentioned
above, to avoid future confusion, this `Store` trait should be renamed to
`Engine` and this document updated to reflect this.

For <handwave> reasons, it actually lives in the
[sync-traits helper](https://github.com/mozilla/application-services/blob/main/components/support/sync15-traits/src/store.rs)
but for the purposes of this document, you should consider it as owned by sync15.

This is actually quite a simple trait - at a high level, it's really just
concerned with:

* Get or set some metadata the sync15 component has decided should be saved or
  fetched.

* In a normal sync, take some "incoming" records, process them, and return
  the "outgoing" records we should send to the server.

* In some edge-cases, either "wipe" (ie, actually delete everything, which
  almost never happens) or "reset" (ie, pretend this engine has never before
  been synced)

And that's it!
