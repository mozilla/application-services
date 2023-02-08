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

use interrupt_support::NeverInterrupts;
use log::*;
use serde_derive::*;
use std::cell::{Cell, RefCell};
use std::mem;
use sync15::bso::OutgoingBso;
use sync15::client::{sync_multiple, MemoryCachedState, Sync15StorageClientInit};
use sync15::engine::{
    CollectionRequest, EngineSyncAssociation, IncomingChangeset, OutgoingChangeset, SyncEngine,
};
use sync15::{telemetry, ServerTimestamp};
use sync_guid::Guid;

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

pub struct TestEngine {
    pub name: &'static str,
    pub test_records: RefCell<Vec<TestRecord>>,
    pub engine_sync_assoc: RefCell<EngineSyncAssociation>,
    pub was_reset_called: Cell<bool>,

    pub global_id: Option<Guid>,
    pub coll_id: Option<Guid>,
}

// Lotsa boilerplate to implement `SyncEngine`... 😅
impl SyncEngine for TestEngine {
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
    ) -> anyhow::Result<OutgoingChangeset> {
        // Notice the `&mut *` and `.borrow_mut()` to extract the Vec from
        // the RefCell.
        let temp: Vec<TestRecord> = mem::take(&mut *self.test_records.borrow_mut());

        let inbound = inbound.into_iter().next().unwrap();
        for bso in inbound.changes {
            let incoming_record = bso.into_content::<TestRecord>().content().unwrap();
            info!("Got incoming record {:?}", incoming_record);

            self.test_records.borrow_mut().push(incoming_record);
        }

        Ok(OutgoingChangeset::new(
            self.collection_name(),
            temp.into_iter()
                .map(OutgoingBso::from_content_with_id)
                .collect::<Result<_, _>>()?,
        ))
    }

    fn sync_finished(
        &self,
        _new_timestamp: ServerTimestamp,
        records_synced: Vec<Guid>,
    ) -> anyhow::Result<()> {
        // This should print something like:
        // `[... INFO sync_test::sync15] Uploaded records: [Guid("ai5xy_LtNAuN")]`
        // If we were a real engine, this is where we'd mark our outgoing records
        // as uploaded. In a test, we can just assert that the records we uploaded
        info!("Uploaded records: {:?}", records_synced);
        Ok(())
    }

    fn get_collection_requests(
        &self,
        _server_timestamp: ServerTimestamp,
    ) -> anyhow::Result<Vec<CollectionRequest>> {
        // This is where we can add a `since` bound, so we only fetch records
        // since the last sync time...but, we aren't storing that yet, so we
        // just fetch all records that we've ever written.
        Ok(vec![CollectionRequest::new(self.collection_name()).full()])
    }

    /// This is where we return our test collection's sync ID (and global sync
    /// ID).
    fn get_sync_assoc(&self) -> anyhow::Result<EngineSyncAssociation> {
        let our_assoc = self.engine_sync_assoc.borrow();
        println!(
            "TEST {}: get_sync_assoc called with {:?}",
            self.name, *our_assoc
        );
        Ok(our_assoc.clone())
    }

    /// Reset the engine without wiping local data, ready for a "first sync".
    /// `assoc` defines how this engine is to be associated with sync.
    fn reset(&self, assoc: &EngineSyncAssociation) -> anyhow::Result<()> {
        println!("TEST {}: Reset called", self.name);
        self.was_reset_called.set(true);
        *self.engine_sync_assoc.borrow_mut() = assoc.clone();
        Ok(())
    }

    // Won't really be used anywhere.
    fn wipe(&self) -> anyhow::Result<()> {
        // This is where we'd erase all data and Sync state. Since we're
        // just an in-memory engine, and `sync_multiple` doesn't exercise
        // this, we do nothing.
        Ok(())
    }
}

fn sync_client(c: &mut TestClient, desc: &str, engine: &dyn SyncEngine) {
    let (auth_info, _device_settings) = c
        .get_sync_data()
        .expect("Should have data for syncing first client");

    let storage_init = &Sync15StorageClientInit {
        key_id: auth_info.kid,
        access_token: auth_info.fxa_access_token,
        tokenserver_url: url::Url::parse(auth_info.tokenserver_url.as_str()).unwrap(),
    };
    let root_sync_key = &sync15::KeyBundle::from_ksync_base64(auth_info.sync_key.as_str()).unwrap();

    let mut persisted_global_state = None;
    let mut mem_cached_state = MemoryCachedState::default();

    let result = sync_multiple(
        &[engine],
        &mut persisted_global_state,
        &mut mem_cached_state,
        storage_init,
        root_sync_key,
        &NeverInterrupts,
        None,
    );
    println!("Finished syncing {desc} client: {:?}", result);
}

fn sync_first_client(c0: &mut TestClient, engine: &dyn SyncEngine) {
    sync_client(c0, "first", engine);
}

fn sync_second_client(c1: &mut TestClient, engine: &dyn SyncEngine) {
    sync_client(c1, "second", engine);
}

// Integration test for the sync15 component
//
// It currently only tests elements and behavior of
// components/sync15/src/sync_multiple.rs
// Note that it will fail if a mock email account cannot be successfully
// created.
fn test_sync_multiple(c0: &mut TestClient, c1: &mut TestClient) {
    let test_vec = vec![TestRecord {
        id: Guid::random(),
        message: "<3".to_string(),
    }];

    let first_client_engine = TestEngine {
        name: "c0",
        test_records: RefCell::new(test_vec.clone()),
        engine_sync_assoc: RefCell::new(EngineSyncAssociation::Disconnected), // should also test Connected
        was_reset_called: Cell::new(false),

        global_id: Option::from(Guid::random()),
        coll_id: Option::from(Guid::random()),
    };
    sync_first_client(c0, &first_client_engine);
    assert!(
        first_client_engine.was_reset_called.get(),
        "Should have called first reset."
    );

    let second_client_engine = TestEngine {
        name: "c1",
        test_records: RefCell::default(),
        engine_sync_assoc: first_client_engine.engine_sync_assoc, // unlike c0, will not call reset()
        was_reset_called: Cell::new(false),

        global_id: Option::from(Guid::random()),
        coll_id: Option::from(Guid::random()),
    };
    sync_second_client(c1, &second_client_engine);
    assert!(
        !second_client_engine.was_reset_called.get(),
        "Second client shouldn't have called reset."
    );

    let vector1 = first_client_engine.test_records.into_inner();
    let vector2 = second_client_engine.test_records.into_inner();

    assert!(vector1.is_empty(), "The vector should be empty.");

    assert_eq!(
        test_vec, vector2,
        "Both clients' messages should match after the two calls to sync_multiple()."
    );
    info!(
        "Client {:?}'s test_records: {:?}",
        first_client_engine.name, vector1
    );
    info!(
        "Client {:?}'s test_records: {:?}",
        second_client_engine.name, vector2
    );
}

// Boilerplate...
pub fn get_test_group() -> TestGroup {
    TestGroup::new("sync15", vec![("test_sync_multiple", test_sync_multiple)])
}
