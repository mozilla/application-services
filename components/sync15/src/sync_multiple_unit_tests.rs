use crate::sync_multiple::{MemoryCachedState};
use crate::client::{Sync15StorageClientInit};
use crate::key_bundle::KeyBundle;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_test() {
        let mut mem_cached_state = MemoryCachedState {

        }
    }

}

// Dummy; empty.
pub struct MockStore {
}
impl Store for MockStore {
}
