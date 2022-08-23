/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(feature = "full-sync")]
pub(crate) mod engine;
#[cfg(feature = "full-sync")]
mod record;

// Our UDL gives the store certain sync-specific functions, but can only be used
// when the `full-sync` feature is enabled - so we must provide stubs when it
// is not.
#[cfg(not(feature = "full-sync"))]
impl crate::TabsStore {
    pub fn reset(self: std::sync::Arc<Self>) -> crate::error::Result<()> {
        log::error!("reset: feature not enabled");
        Err(crate::error::TabsError::SyncAdapterError(
            "reset".to_string(),
        ))
    }

    pub fn sync(
        self: std::sync::Arc<Self>,
        _key_id: String,
        _access_token: String,
        _sync_key: String,
        _tokenserver_url: String,
        _local_id: String,
    ) -> crate::error::Result<String> {
        log::error!("sync: feature not enabled");
        Err(crate::error::TabsError::SyncAdapterError(
            "sync".to_string(),
        ))
    }

    pub fn register_with_sync_manager(self: std::sync::Arc<Self>) {
        log::error!("register_with_sync_manager: feature not enabled");
    }
}
