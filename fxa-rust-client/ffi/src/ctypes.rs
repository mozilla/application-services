use libc::c_char;
use fxa_client::SyncKeys;
use util::*;

#[repr(C)]
pub struct SyncKeysC {
    pub sync_key: *mut c_char,
    pub xcs: *mut c_char
}

impl From<SyncKeys> for SyncKeysC {
    fn from(sync_keys: SyncKeys) -> Self {
        SyncKeysC {
            sync_key: string_to_c_char(sync_keys.0),
            xcs: string_to_c_char(sync_keys.1)
        }
    }
}