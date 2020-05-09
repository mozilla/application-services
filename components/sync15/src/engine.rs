use crate::{MemoryCachedState};

use log::*;
use serde_derive::*;
use sync15_traits::{Store, IncomingChangeset, OutgoingChangeset, telemetry, Payload, ServerTimestamp, CollectionRequest, StoreSyncAssociation, CollSyncIds};
use failure::Error;
use sync_guid::Guid;
use std::cell::{RefCell, Cell, Ref};
use std::borrow::Borrow;

// A test record. It has to derive `Serialize` and `Deserialize` (which we import
// in scope when we do `use serde_derive::*`), so that the `sync15` crate can
// serialize them to JSON, and parse them from JSON. Deriving `Debug` lets us
// print it with `{:?}` below.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TestRecord {
    // This field is required for all Sync records, but can be set to whatever
    // value we want. In the test, we just generate a random GUID.
    pub id: Guid,
    // And a field to our record.
    pub message: String,
}


///   To be used in the sync15 integration test   ///
// A test store, that doesn't hold on to any state (yet!)
pub struct TestStore {
    pub name: &'static str,
    pub test_records: RefCell<Vec<TestRecord>>,
    pub store_sync_assoc: RefCell<StoreSyncAssociation>,
    pub was_reset_called: Cell<bool>,

    pub global_id: Option<Guid>,
    pub coll_id: Option<Guid>,
}

// Lotsa boilerplate to implement `Store`... 😅
impl Store for TestStore {
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        // HACK: We have to use a "well-known" collection name in `meta/global`
        // (you can see the list in `DEFAULT_ENGINES`, inside
        // `components/sync15/src/state.rs`). Otherwise, `LocalCollStateMachine`
        // won't know how to set up the sync IDs. Even though `TestRecord` isn't
        // actually an address record, that's OK—they're all encrypted, so the
        // server can't check their contents.
        "addresses".into()
    }

    fn apply_incoming(
        &self,
        inbound: Vec<IncomingChangeset>,
        _telem: &mut telemetry::Engine,
    ) -> Result<OutgoingChangeset, Error> {
        let inbound = inbound.into_iter().next().unwrap();
        for (payload, _timestamp) in inbound.changes {
            // Here's an example of a magic "into" conversion that we define
            // ourselves. If you look inside
            // `/components/support/sync15-traits/src/payload.rs`, you'll see
            // a declaration like:
            //
            //     fn into_record<T>(self) -> Result<T, serde_json::Error>
            //     where
            //         for<'a> T: Deserialize<'a>
            //
            // That means `into_record` returns any type `T` that implements
            // the `Deserialize` trait—which `TestRecord` does! (The `for<'a>`
            // thing is a "higher-ranked trait bound", and means that the
            // function is generic over any lifetime that `T` has. We need
            // that because serde's `Deserialize` trait has a lifetime parameter
            // `'de` here: https://docs.serde.rs/serde/de/trait.Deserialize.html
            // ...but we don't care what it is. You can read it like "where T
            // implements `Deserialize` for any lifetime 'a").
            //
            // But what happens if we can't actually deserialize the payload into
            // T? (Let's say T has a required field that the payload doesn't have).
            // That's why `into_record` returns a `Result<T, Error>` instead of just
            // `T`.
            let incoming_record: TestRecord = payload.into_record()?;

            // DONE: Push `incoming_record` into `self.test_record`.
            // TODO: BIG QUESTION
            //self.test_records.into_inner().push(incoming_record);
            self.test_records.replace(vec![incoming_record.clone()]);

            // `info!` is a macro from the `log` crate. It's like `println!`,
            // except it'll give us a green "INFO" line, and also let us filter
            // them out with the `RUST_LOG` environment variable.
            info!("Got incoming record {:?}", incoming_record);
        }

        /* Doing it from the integration test file (sync15.rs) now.
        // Let's make an outgoing record to upload...
        let outgoing_record = TestRecord {
            // Random for now, but we can use any GUID we want...
            // `"recordAAAAAA".into()` also works!
            id: Guid::random(),
            message: (self.test_record).clone()
        };
        */
        // Could use `*` to extract the TestRecord from the Ref.
        let temp_outgoing_record = self.test_records.borrow();

        let outgoing_record: Result<Vec<Payload>, serde_json::error::Error> =
            temp_outgoing_record
                .iter()
                .map(|t| Payload::from_record(t.clone())) // CHECK: clone() correct here? No `&` in |t| correct?
                .collect();

        // CHECK: Should we use .is_ok() ?
        let outgoing_record = outgoing_record.unwrap();

        let mut outgoing = OutgoingChangeset::new(self.collection_name(), inbound.timestamp);

        // DONE: Turn `outgoing_record` (which is a `&Vec<TestRecord>`)
        //       into a `Vec<Payload>`, so that we can assign it to
        //       `outgoing.changes`.
        for record in outgoing_record {
            outgoing
                .changes
                .push(record); // CHECK: not using `?`
        }

        Ok(outgoing)
    }

    fn sync_finished(
        &self,
        _new_timestamp: ServerTimestamp,
        records_synced: Vec<Guid>,
    ) -> Result<(), Error> {
        // This should print something like:
        // `[... INFO sync_test::sync15] Uploaded records: [Guid("ai5xy_LtNAuN")]`
        // If we were a real store, this is where we'd mark our outgoing records
        // as uploaded. In a test, we can just assert that the records we uploaded
        info!("Uploaded records: {:?}", records_synced);
        Ok(())
    }

    fn get_collection_requests(
        &self,
        _server_timestamp: ServerTimestamp,
    ) -> Result<Vec<CollectionRequest>, Error> {
        // This is where we can add a `since` bound, so we only fetch records
        // since the last sync time...but, we aren't storing that yet, so we
        // just fetch all records that we've ever written.
        Ok(vec![CollectionRequest::new(self.collection_name()).full()])
    }

    // If we held on to the collection's sync ID (and global sync ID),
    // this is where we'd return them...but, for now, we just pretend
    // like it's a first sync.
    // [DONE]
    fn get_sync_assoc(&self) -> Result<StoreSyncAssociation, Error> {
        let our_assoc = self.store_sync_assoc.borrow();
        println!(
            "TEST {}: get_sync_assoc called with {:?}",
            self.name, *our_assoc
        );
        Ok(our_assoc.clone())

        /* KEEP?
        let global = (self.global_id).clone();
        let coll = (self.coll_id).clone();

        if let (Some(global), Some(coll)) = (global, coll) {
            Ok(StoreSyncAssociation::Connected(CollSyncIds {global, coll }))
        } else {
            Ok(StoreSyncAssociation::Disconnected)
        }
        */
    }

    /// Reset the store without wiping local data, ready for a "first sync".
    /// `assoc` defines how this store is to be associated with sync.
    fn reset(&self, assoc: &StoreSyncAssociation) -> Result<(), Error> {
        // If we held on to any state, this is where we'd drop it, and replace
        // it with what we were given in `assoc`. But we don't, so we do
        // nothing.
        println!("TEST {}: Reset called", self.name);
        self.was_reset_called.set(true);
        *self.store_sync_assoc.borrow_mut() = assoc.clone();

        // Are we resetting the `id` or the `message` of the TestRecord?
        /* COPIED from components/places/src/bookmark_sync/store.rs
        match assoc {
            //local data
            StoreSyncAssociation::Disconnected => {
                reset(self.db)?;  // defined in components/places/src/storage/bookmarks.rs
            }
            //sync data
            StoreSyncAssociation::Connected(ids) => {
                let tx = self.db.begin_transaction()?;
                reset_meta(self.db)?;
                put_meta(self.db, GLOBAL_SYNCID_META_KEY, &ids.global)?;
                put_meta(self.db, COLLECTION_SYNCID_META_KEY, &ids.coll)?;
                tx.commit()?;
            }
        }
        */
        Ok(())
    }

    // WON'T really be used anywhere.
    fn wipe(&self) -> Result<(), Error> {
        // This is where we'd erase all data and Sync state. Since we're
        // just an in-memory store, and `sync_multiple` doesn't exercise
        // this, we do nothing.
        Ok(())
    }
}


//// They really want to move away from testing like this (using engines!) ////

pub struct SyncMultipleStorage {
    local_stores: RefCell<Option<Vec<TestStore>>>,
    //remote_stores: RefCell<Option<Vec<TestStore>>>,
}

impl SyncMultipleStorage {
    pub fn new() -> Self {
        Self {
            local_stores: RefCell::default(),
        }
    }
}

pub struct SyncMultipleEngine {
    pub storage: SyncMultipleStorage,
    pub mem_cached_state: Cell<MemoryCachedState>,
}

impl SyncMultipleEngine {
    pub fn new() -> Self {
        Self {
            storage: SyncMultipleStorage::new(),
            mem_cached_state: Cell::default(),
        }
    }
}