# Sync Overview

This document provides a high-level overview of how syncing works.  **Note**: each component has its own quirks and will handle sync slightly differently than the general process described here.

## General flow and architecture

- **Crates involved**:
  - The `sync15` and `support/sync15-traits` handle the general syncing logic and define the `SyncEngine` trait
  - Individual component crates (`logins`, `places`, `autofill`, etc).  These implement `SyncEngine`.
  - `sync_manager` manages the overall syncing process.
- **High level sync flow**:
  - Sync is initiated by the application that embeds application-services.
  - The application calls `SyncManager.sync()` to start the sync process.
  - `SyncManager` creates `SyncEngine` instances to sync the individual components.  Each `SyncEngine` corresponds to a `collection` on the sync server.

### Sync manager

[`SyncManager`](https://github.com/mozilla/application-services/blob/main/components/sync_manager/src/manager.rs) is responsible for performing the high-level parts of the sync process:
  - The consumer code calls it's `sync()` function to start the sync, passing
    in a [`SyncParams`](https://mozilla.github.io/application-services/rust-docs/sync_manager/msg_types/struct.SyncParams.html) object in, which describes what should be synced.
  - `SyncManager` performs all network operations on behalf of the individual engines. It's also responsible for tracking the general authentication state (primarily by inspecting the responses from these network requests) and fetching tokens from the token server.
  - `SyncManager` checks if we are currently in a backoff period and should wait before contacting the server again.
  - Before syncing any engines, the sync manager checks the state of the meta/global collection and compares it with the enabled engines specified in the SyncParams.  This handles the cases when the user has requested an engine be enabled or disabled on this device, or when it was requested on a different device. (Note that engines enabled and disabled states are state on the account itself and not a per-device setting).  Part of this process is comparing the collection's GUID on the server with the GUID known locally - if they are different, it implies some other device has "reset" the collection, so the engine drops all metadata and attempts to reconcile with every record on the server (ie, acts as though this is the very first sync this engine has ever done).
  - `SyncManager` instantiates a `SyncEngine` for each enabled component.  We currently use 2 different methods for this:
    - The older method is for the `SyncManager` to hold a weakref to a `Store` use that to create the `SyncEngine` (tabs and places).  The `SyncEngine` uses the `Store` for database access, see the [`TabsStore`](https://mozilla.github.io/application-services/rust-docs/tabs/struct.TabsStore.html) for an example.
    - The newer method is for the components to provide a function to create the `SyncEngine`, hiding the details of how that engine gets created (autofill/logins).  These components also define a `Store` instance for the `SyncEngine` to use, but it's all transparent to the `SyncManager`.  (See [`autofill::get_registered_sync_engine()`](https://mozilla.github.io/application-services/rust-docs/autofill/db/store/fn.get_registered_sync_engine.html) and [`autofill::db::store::Store`](https://mozilla.github.io/application-services/rust-docs/autofill/db/store/struct.Store.html))
  - For components that use local encryption, `SyncManager` passes the local encryption key to their `SyncEngine`
  - Finally, calls `sync_multiple()` function from the `sync15` crate, sending it the `SyncEngine` instances.  `sync_multiple()` then calls the `sync()` function for each individual `SyncEngine`


### Sync engines
  - [`SyncEngine`](https://github.com/mozilla/application-services/blob/main/components/support/sync15-traits/src/engine.rs) is defined in the `support/sync15-traits` crate and defines the interface for syncing a component.
  - A new `SyncEngine` instance is created for each sync
  - `SyncEngine.apply_incoming()` does the main work.  It is responsible for processing incoming records from the server in order to update the local records and calculating which local records should be synced back.

## The `apply_incoming` pattern

`SyncEngine` instances are free to implement `apply_incoming()` any way they want, but the most components follow a general pattern.

### Database Tables

   - The local table stores records for the local application
   - The mirror table stores the last known record from the server
   - The staging temporary table stores the incoming records that we're currently processing
   - The local/mirror/staging tables contains a `guid` as its primary key.  A record will share the same `guid` for the local/mirror/staging table.
   - The metadata table stores the GUID for the collection as a whole and the the last-known server timestamp of the collection.

### `apply_incoming` stages
  - **stage incoming**: write out each incoming server record to the staging table
  - **fetch states**: take the rows from all 3 tables and combine them into a single struct containing `Option`s for the local/mirror/staging records.
  - **iterate states**: loop through each state, decide how to do change the local records, then execute that plan.
    - **reconcile/plan**: For each state we create an action plan for it.  The action plan is a low-level description of what to change (add this record, delete this one, modify this field, etc).  Here are some common situations:
       - **A record only appears in the staging table**.  It's a new record from the server and should be added to the local DB
       - **A record only appears in the local table**.  It's a new record on the local instance and should be synced back to the serve
       - **Identical records appear in the local/mirror tables and a changed record is in the staging table**.  The record was updated remotely and the changes should be propagated to the local DB.
       - **A record appears in the mirror table and changed records appear in both the local and staging tables**.  The record was updated both locally and remotely and we should perform a 3-way merge.
    - **apply plan**: After we create the action plan, then we execute it.
  - **fetch outgoing**:
     - Calculate which records need to be sent back to the server
     - Update the mirror table
     - Return those records back to the `sync15` code so that it can upload them to the server.
     - The sync15 code returns the timestamp reported by the server in the POST response and hands it back to the engine. The engine persists this timestamp in the metadata table - the next sync will then use this timestamp to only fetch records that have since been changed by other devices

### syncChangeCounter
The local table has an integer column syncChangeCounter which is incremented every time the embedding app makes a change to a local record (eg, updating a field). Thus, any local record with a non-zero change counter will need to be updated on the server (with either the local record being used, or after it being merged if the record also changed remotely). At the start of the sync, when we are determining what action to take, we take a copy of the change counter, typically in a temp staging table. After we have uploaded the record to the server, we decrement the counter by whatever it was when the sync started. This means that if a record is changed in between staging the record and uploading it, the change counter will not drop to zero, and so it will correctly be seen as locally modified on the next sync
