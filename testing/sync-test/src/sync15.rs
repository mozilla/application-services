/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */
//
// To compile/run from the root directory:
//
//     cargo check -p sync-test
//     cargo run -p sync-test -- --oauth-retries 5
//
// (You can safely ignore the noisy 500 for
// `https://stable.dev.lcip.org/auth/v1/account/destroy` at the end).

use log::*;
use serde_derive::*;
use failure::Error;
use sync_guid::{Guid};
use sync15_traits::{Store, CollectionRequest, IncomingChangeset, OutgoingChangeset, Payload, ServerTimestamp, StoreSyncAssociation, CollSyncIds};
use sync15::{telemetry, MemoryCachedState};
use interrupt::NeverInterrupts;
use std::cell::{RefCell, Cell};
use std::borrow::{BorrowMut, Borrow};
use std::mem;

use crate::auth::TestClient;
use crate::testing::TestGroup;

// A test record. It has to derive `Serialize` and `Deserialize` (which we import
// in scope when we do `use serde_derive::*`), so that the `sync15` crate can
// serialize them to JSON, and parse them from JSON. Deriving `Debug` lets us
// print it with `{:?}` below.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct TestRecord {
    // This field is required for all Sync records, but can be set to whatever
    // value we want. In the test, we just generate a random GUID, but we can
    //  use any GUID we want...
    // `"recordAAAAAA".into()` also works!
    pub id: Guid,
    // To test that syncing happens.
    pub message: String,
}

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
        // Notice the `&mut *` and `.borrow_mut()` to extract the Vec from
        // the RefCell.
        let temp: Vec<TestRecord> = mem::take(&mut *self.test_records.borrow_mut());

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

            // `info!` is a macro from the `log` crate. It's like `println!`,
            // except it'll give us a green "INFO" line, and also let us filter
            // them out with the `RUST_LOG` environment variable.
            info!("Got incoming record {:?}", incoming_record);

            self.test_records.borrow_mut().push(incoming_record);
        }

        let outgoing_record: Result<Vec<Payload>, serde_json::error::Error> =
            temp
                .into_iter() // dereferences the TestRecord (`t` below)
                .map(|t| Payload::from_record(t))
                .collect();

        let outgoing_record = outgoing_record?;

        let mut outgoing = OutgoingChangeset::new(self.collection_name(), inbound.timestamp);

        for record in outgoing_record {
            outgoing
                .changes
                .push(record);
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
    // like it's a first sync. [DONE]
    fn get_sync_assoc(&self) -> Result<StoreSyncAssociation, Error> {
        let our_assoc = self.store_sync_assoc.borrow();
        println!(
            "TEST {}: get_sync_assoc called with {:?}",
            self.name, *our_assoc
        );
        Ok(our_assoc.clone())

        /* KEEP: could also be helpful?
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

        /* Do we want to reset the `id` or the `message` of the TestRecord?
        // COPIED from components/places/src/bookmark_sync/store.rs
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

    // Won't really be used anywhere.
    fn wipe(&self) -> Result<(), Error> {
        // This is where we'd erase all data and Sync state. Since we're
        // just an in-memory store, and `sync_multiple` doesn't exercise
        // this, we do nothing.
        Ok(())
    }
}


fn sync_first_client(c0: &mut TestClient, store: &dyn Store) {
    let (init, key, _device_id) = c0
        .data_for_sync()
        .expect("Should have data for syncing first client");

    let mut persisted_global_state = None;
    let mut mem_cached_state = MemoryCachedState::default();

    let result = sync15::sync_multiple(
        &[store],
        &mut persisted_global_state,
        &mut mem_cached_state,
        &init,
        &key,
        &NeverInterrupts,
        None,
    );

    println!("Finished syncing first client: {:?}", result);
}

fn sync_second_client(c1: &mut TestClient, store: &dyn Store) {
    let (init, key, _device_id) = c1
        .data_for_sync()
        .expect("Should have data for syncing second client");

    let mut persisted_global_state = None;
    let mut mem_cached_state = MemoryCachedState::default();

    let result = sync15::sync_multiple(
        &[store],
        &mut persisted_global_state,
        &mut mem_cached_state,
        &init,
        &key,
        &NeverInterrupts,
        None,
    );

    println!("Finished syncing second client: {:?}", result);
}

// Integration test for the sync15 component //
//
// It currently only tests elements and behavior of
// components/sync15/src/sync_multiple.rs
// Note that it will fail if a mock email account cannot be successfully
// created.
fn test_sync_multiple(c0: &mut TestClient, c1: &mut TestClient) {
    let test_vec = vec![
        TestRecord {
            id: Guid::random(),
            message: "<3".to_string()
        }
    ];

    let first_client_store = TestStore {
        name: "c0",
        test_records: RefCell::new(test_vec.clone()),
        store_sync_assoc: RefCell::new(StoreSyncAssociation::Disconnected), // should also test Connected
        was_reset_called: Cell::new(false),

        global_id: Option::from(Guid::random()),
        coll_id: Option::from(Guid::random())
    };
    sync_first_client(c0, &first_client_store);
    assert_eq!(
        first_client_store.was_reset_called.get(),
        true,
        "Should have called first reset."
    );

    let second_client_store = TestStore {
        name: "c1",
        test_records: RefCell::default(),
        store_sync_assoc: first_client_store.store_sync_assoc, // unlike c0, will not call reset()
        was_reset_called: Cell::new(false),

        global_id: Option::from(Guid::random()),
        coll_id: Option::from(Guid::random())
    };
    sync_second_client(c1, &second_client_store);
    assert_eq!(
        second_client_store.was_reset_called.get(),
        false,
        "Second client shouldn't have called reset."
    );

    let vector1 = first_client_store.test_records.into_inner();
    let vector2 = second_client_store.test_records.into_inner();

    assert!(vector1.is_empty(),
            "The vector should be empty.");

    assert_eq!(test_vec, vector2,
               "Both clients' messages should match after the two calls to sync_multiple().");
    info!("Client {:?}'s test_records: {:?}", first_client_store.name, vector1);
    info!("Client {:?}'s test_records: {:?}", second_client_store.name, vector2);
}

// Boilerplate...
pub fn get_test_group() -> TestGroup {
    TestGroup::new("sync15",
                   vec![("test_sync_multiple", test_sync_multiple)])
}
