/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

use crate::auth::TestClient;
use crate::testing::TestGroup;
use sync15::{Store, Error, Sync15StorageClientInit, CollectionRequest, RecordChangeset, Payload, ServerTimestamp, StoreSyncAssociation};
use std::borrow::Cow;
use sync15::telemetry::Engine;

fn basic_test() {
    let mut store1 = MockStore{};
    let mut store2 = MockStore{};
    let mut fake_client = TestClient{
        fxa: (),
        test_acct: Arc::new(TestAccount {}),
        logins_engine: PasswordEngine {},
        tabs_engine: Default::default()
    };

    

}

pub struct MockStore {
}
impl Store for MockStore {
    fn collection_name(&self) -> Cow<'static, str> {
        unimplemented!()
    }

    fn apply_incoming(&self, inbound: Vec<RecordChangeset<(Payload, ServerTimestamp)>>, telem: &mut Engine) -> Result<RecordChangeset<Payload>, Error> {
        unimplemented!()
    }

    fn sync_finished(&self, new_timestamp: ServerTimestamp, records_synced: Vec<Guid>) -> Result<(), Error> {
        unimplemented!()
    }

    fn get_collection_requests(&self, server_timestamp: ServerTimestamp) -> Result<Vec<CollectionRequest>, Error> {
        unimplemented!()
    }

    fn get_sync_assoc(&self) -> Result<StoreSyncAssociation, Error> {
        unimplemented!()
    }

    fn reset(&self, assoc: &StoreSyncAssociation) -> Result<(), Error> {
        unimplemented!()
    }

    fn wipe(&self) -> Result<(), Error> {
        unimplemented!()
    }
}


// Actual tests.

// But in truth, only the sync_multiple file (function?) is being tested.
// This function is given two clients to
fn test_sync15(c0: &mut TestClient, c1: &mut TestClient) {

}


pub fn get_test_group() -> TestGroup {
    TestGroup::new("sync15",
                   vec![("test_sync15", test_sync15)])
}
