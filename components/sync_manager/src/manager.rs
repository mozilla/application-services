/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::msg_types::{ServiceStatus, SyncParams, SyncReason, SyncResult};
use logins::PasswordEngine;
use places::{bookmark_sync::store::BookmarksStore, history_sync::store::HistoryStore, PlacesApi};
use std::collections::HashMap;
use std::sync::{atomic::AtomicUsize, Arc, Mutex, Weak};
use std::time::SystemTime;
use sync15::MemoryCachedState;

const LOGINS_ENGINE: &str = "passwords";
const HISTORY_ENGINE: &str = "history";
const BOOKMARKS_ENGINE: &str = "bookmarks";

pub struct SyncManager {
    mem_cached_state: Option<MemoryCachedState>,
    places: Weak<PlacesApi>,
    logins: Weak<Mutex<PasswordEngine>>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            mem_cached_state: None,
            places: Weak::new(),
            logins: Weak::new(),
        }
    }

    pub fn set_places(&mut self, places: Arc<PlacesApi>) {
        self.places = Arc::downgrade(&places);
    }

    pub fn set_logins(&mut self, logins: Arc<Mutex<PasswordEngine>>) {
        self.logins = Arc::downgrade(&logins);
    }

    pub fn disconnect(&mut self) {
        if let Some(logins) = self
            .logins
            .upgrade()
            .as_ref()
            .map(|l| l.lock().expect("poisoned logins mutex"))
        {
            if let Err(e) = logins.reset() {
                log::error!("Failed to reset logins: {}", e);
            }
        } else {
            log::warn!("Unable to wipe logins, be sure to call set_logins before disconnect if this is surprising");
        }

        if let Some(places) = self.places.upgrade() {
            if let Err(e) = places.reset_bookmarks() {
                log::error!("Failed to reset bookmarks: {}", e);
            }
            if let Err(e) = places.reset_history() {
                log::error!("Failed to reset history: {}", e);
            }
        } else {
            log::warn!("Unable to wipe places, be sure to call set_places before disconnect if this is surprising");
        }
    }

    pub fn sync(&mut self, params: SyncParams) -> Result<SyncResult> {
        let mut have_engines = vec![];
        let places = self.places.upgrade();
        let logins = self.logins.upgrade();
        if places.is_some() {
            have_engines.push(HISTORY_ENGINE);
            have_engines.push(BOOKMARKS_ENGINE);
        }
        if logins.is_some() {
            have_engines.push(LOGINS_ENGINE);
        }
        check_engine_list(&params.engines_to_sync, &have_engines)?;

        let next_sync_after = self
            .mem_cached_state
            .as_ref()
            .and_then(|mcs| mcs.get_next_sync_after());
        if backoff_in_effect(next_sync_after, &params) {
            self.do_sync(params)
        } else {
            Ok(SyncResult {
                status: ServiceStatus::BackedOff as i32,
                results: Default::default(),
                have_declined: false,
                declined: vec![],
                next_sync_allowed_at: system_time_to_millis(next_sync_after),
                persisted_state: params.persisted_state.unwrap_or_default(),
                // It would be nice to record telemetry here.
                telemetry_json: None,
            })
        }
    }

    fn do_sync(&mut self, mut params: SyncParams) -> Result<SyncResult> {
        let mut places = self.places.upgrade();
        let logins = self.logins.upgrade();

        let key_bundle = sync15::KeyBundle::from_ksync_base64(&params.acct_sync_key)?;
        let tokenserver_url = url::Url::parse(&params.acct_tokenserver_url)?;

        let logins_sync = should_sync(&params, LOGINS_ENGINE);
        let bookmarks_sync = should_sync(&params, BOOKMARKS_ENGINE);
        let history_sync = should_sync(&params, HISTORY_ENGINE);

        let places_conn = if bookmarks_sync || history_sync {
            places
                .as_mut()
                .expect("already checked")
                .open_sync_connection()
                .ok()
        } else {
            None
        };
        let l = logins.as_ref().map(|l| l.lock().expect("poisoned mutex"));
        // TODO(issue 1684) this isn't ideal, we should have real support for interruption.
        let p = Arc::new(AtomicUsize::new(0));
        let interruptee = sql_support::SqlInterruptScope::new(p);

        let mut mem_cached_state = self.mem_cached_state.take().unwrap_or_default();
        let mut disk_cached_state = params.persisted_state.take();
        // `sync_multiple` takes a &[&dyn Store], but we need something to hold
        // ownership of our stores.
        let mut stores: Vec<Box<dyn sync15::Store>> = vec![];

        if let Some(pc) = places_conn.as_ref() {
            if history_sync {
                stores.push(Box::new(HistoryStore::new(pc, &interruptee)))
            }
            if bookmarks_sync {
                stores.push(Box::new(BookmarksStore::new(pc, &interruptee)))
            }
        }

        if let Some(le) = l.as_ref() {
            assert!(logins_sync, "Should have already checked");
            stores.push(Box::new(logins::LoginStore::new(&le.db)));
        }

        let store_refs: Vec<&dyn sync15::Store> = stores.iter().map(|s| &**s).collect();

        let client_init = sync15::Sync15StorageClientInit {
            key_id: params.acct_key_id.clone(),
            access_token: params.acct_access_token.clone(),
            tokenserver_url,
        };

        let result = sync15::sync_multiple(
            &store_refs,
            &mut disk_cached_state,
            &mut mem_cached_state,
            &client_init,
            &key_bundle,
            &interruptee,
            Some(&params.engines_to_change_state),
        );

        let status = ServiceStatus::from(result.service_status) as i32;
        let results: HashMap<String, String> = result
            .engine_results
            .into_iter()
            .map(|(e, r)| {
                (
                    e,
                    match r {
                        Ok(()) => "".to_string(),
                        Err(e) => {
                            let msg = e.to_string();
                            if msg.is_empty() {
                                log::error!(
                                    "Bug: error message string is empty for error: {:?}",
                                    e
                                );
                                // This shouldn't happen, but we use empty string to
                                // indicate success on the other side, so just ensure
                                // our errors error can't be
                                "<unspecified error>".to_string()
                            } else {
                                msg
                            }
                        }
                    },
                )
            })
            .collect();

        // Unwrap here can never fail -- it indicates trying to serialize an
        // unserializable type.
        let telemetry_json = serde_json::to_string(&result.telemetry).unwrap();

        Ok(SyncResult {
            status,
            results,
            have_declined: result.declined.is_some(),
            declined: result.declined.unwrap_or_default(),
            next_sync_allowed_at: system_time_to_millis(result.next_sync_after),
            persisted_state: disk_cached_state.unwrap_or_default(),
            telemetry_json: Some(telemetry_json),
        })
    }
}

fn backoff_in_effect(next_sync_after: Option<SystemTime>, p: &SyncParams) -> bool {
    let now = SystemTime::now();
    if let Some(nsa) = next_sync_after {
        if nsa > now {
            return if p.reason == (SyncReason::User as i32)
                || p.reason == (SyncReason::EnabledChange as i32)
            {
                log::info!(
                    "Still under backoff, but syncing anyway because reason is {:?}",
                    p.reason
                );
                false
            } else if !p.engines_to_change_state.is_empty() {
                log::info!(
                    "Still under backoff, but syncing because we have enabled state changes."
                );
                false
            } else {
                log::info!(
                    "Still under backoff, and there's no compelling reason for us to ignore it"
                );
                true
            };
        }
    }
    log::debug!("Not under backoff");
    false
}

impl From<sync15::ServiceStatus> for ServiceStatus {
    fn from(s15s: sync15::ServiceStatus) -> Self {
        use sync15::ServiceStatus::*;
        match s15s {
            Ok => ServiceStatus::Ok,
            NetworkError => ServiceStatus::NetworkError,
            ServiceError => ServiceStatus::ServiceError,
            AuthenticationError => ServiceStatus::AuthError,
            BackedOff => ServiceStatus::BackedOff,
            Interrupted => ServiceStatus::OtherError, // Eh...
            OtherError => ServiceStatus::OtherError,
        }
    }
}

fn system_time_to_millis(st: Option<SystemTime>) -> Option<i64> {
    use std::convert::TryFrom;
    let d = st?.duration_since(std::time::UNIX_EPOCH).ok()?;
    // This should always succeed for remotely sane values.
    i64::try_from(d.as_secs() * 1_000 + u64::from(d.subsec_nanos()) / 1_000_000).ok()
}

fn should_sync(p: &SyncParams, engine: &str) -> bool {
    p.sync_all_engines || p.engines_to_sync.iter().any(|e| e == engine)
}

fn check_engine_list(list: &[String], have_engines: &[&str]) -> Result<()> {
    for e in list {
        if e == "bookmarks" || e == "history" || e == "passwords" {
            if !have_engines.iter().any(|engine| e == engine) {
                return Err(ErrorKind::UnsupportedFeature(e.to_string()).into());
            }
        } else {
            return Err(ErrorKind::UnknownEngine(e.to_string()).into());
        }
    }
    Ok(())
}
