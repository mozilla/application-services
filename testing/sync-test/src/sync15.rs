// To run from the root directory:
//
//     cargo run -p sync-test -- --oauth-retries 5
//
// (You can safely ignore the noisy 500 for
// `https://stable.dev.lcip.org/auth/v1/account/destroy` at the end).

use failure::Error;
use interrupt::NeverInterrupts;
use log::*;
use serde_derive::*;
use sync15::{telemetry, CollectionRequest, IncomingChangeset, MemoryCachedState, OutgoingChangeset, Payload, ServerTimestamp, Store, StoreSyncAssociation, TestRecord, TestStore};
use sync_guid::{Guid};
use std::cell::{RefCell, Cell};
use std::borrow::{BorrowMut, Borrow};
use sync15_traits::{CollSyncIds}; // had to declare this dependency in component's Cargo.toml

use crate::auth::TestClient;
use crate::testing::TestGroup;

fn sync_first_client(c0: &mut TestClient, store: &Store) {
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

fn sync_second_client(c1: &mut TestClient, store: &Store) {
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

// Integration test.
// Note that it will fail if a mock email account cannot be successfully created.
fn test_sync_multiple(c0: &mut TestClient, c1: &mut TestClient) {
    let first_client_store = TestStore {
        name: "c0",
        test_records: RefCell::new(vec![
            TestRecord {
                id: Guid::random(),
                message: "<3".to_string()
            }
        ]),
        store_sync_assoc: RefCell::new(StoreSyncAssociation::Disconnected), // also test Connected !
        was_reset_called: Cell::new(false),

        global_id: Option::from(Guid::random()),
        coll_id: Option::from(Guid::random())
    };
    sync_first_client(c0, &first_client_store);
    assert_eq!(
        first_client_store.was_reset_called.get(),
        true,
        "Should have called first reset"
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
        "Second client shouldn't have called reset"
    );
    // DONE: Assert that we uploaded our test record.
    let vector1 = first_client_store.test_records.into_inner();
    let vector2 = second_client_store.test_records.into_inner();
    for i in 0..vector1.len() {
        let first_record = vector1[i].clone();
        let second_record = vector2[i].clone();

        info!("Client {:?}'s test_records[{:?}]: {:?}", first_client_store.name, i, first_record.message);
        info!("Client {:?}'s test_records[{:?}]: {:?}", second_client_store.name, i, second_record.message);
        assert_eq!(first_record.message, second_record.message, "Messages are not synced after two calls to sync_multiple()");
        //drop(first_record);
        //drop(second_record);
    }
}

// Boilerplate...
pub fn get_test_group() -> TestGroup {
    TestGroup::new("sync15",
                   vec![("test_sync_multiple", test_sync_multiple)])
}
