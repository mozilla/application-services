/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub(crate) mod bridge;
pub(crate) mod engine;
mod record;

#[cfg(feature = "full-sync")]
pub mod full_sync;

// When full-sync isn't enabled we need stub versions for these UDL exposed functions.
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
}
