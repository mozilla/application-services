/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::msg_types::{DeviceType, ServiceStatus, SyncParams, SyncReason, SyncResult};
use crate::{reset, reset_all, wipe};
use std::collections::{HashMap, HashSet};
use std::sync::{atomic::AtomicUsize, Arc, Mutex};
use std::time::SystemTime;
use sync15::{
    self,
    clients::{self, Command, CommandProcessor, CommandStatus, Settings},
    EngineSyncAssociation, MemoryCachedState, SyncEngine,
};

const LOGINS_ENGINE: &str = "passwords";
const HISTORY_ENGINE: &str = "history";
const BOOKMARKS_ENGINE: &str = "bookmarks";
const TABS_ENGINE: &str = "tabs";
const ADDRESSES_ENGINE: &str = "addresses";
const CREDIT_CARDS_ENGINE: &str = "creditcards";

// Casts aren't allowed in `match` arms, so we can't directly match
// `SyncParams.device_type`, which is an `i32`, against `DeviceType`
// variants. Instead, we reflect all variants into constants, cast them
// into the target type, and match against them. Please keep this list in sync
// with `msg_types::DeviceType` and `sync15::clients::DeviceType`.
const DEVICE_TYPE_DESKTOP: i32 = DeviceType::Desktop as i32;
const DEVICE_TYPE_MOBILE: i32 = DeviceType::Mobile as i32;
const DEVICE_TYPE_TABLET: i32 = DeviceType::Tablet as i32;
const DEVICE_TYPE_VR: i32 = DeviceType::Vr as i32;
const DEVICE_TYPE_TV: i32 = DeviceType::Tv as i32;

#[derive(Default)]
pub struct SyncManager {
    mem_cached_state: Mutex<Option<MemoryCachedState>>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn autofill_engine(engine: &str) -> Option<Box<dyn SyncEngine>> {
        autofill::get_registered_sync_engine(engine)
    }

    pub fn logins_engine(engine: &str) -> Option<Box<dyn SyncEngine>> {
        logins::get_registered_sync_engine(engine)
    }

    pub fn places_engine(engine: &str) -> Option<Box<dyn SyncEngine>> {
        places::get_registered_sync_engine(engine)
    }

    pub fn tabs_engine(engine: &str) -> Option<Box<dyn SyncEngine>> {
        tabs::get_registered_sync_engine(engine)
    }

    pub fn wipe(&self, engine: &str) -> Result<()> {
        match engine {
            "logins" => {
                if let Some(engine) = Self::logins_engine(engine) {
                    engine.wipe()?;
                }
                Ok(())
            }
            "bookmarks" => {
                if let Some(engine) = Self::places_engine(engine) {
                    engine.wipe()?;
                }
                Ok(())
            }
            "history" => {
                if let Some(engine) = Self::places_engine(engine) {
                    engine.wipe()?;
                }
                Ok(())
            }
            "addresses" | "creditcards" => {
                if let Some(engine) = Self::autofill_engine(engine) {
                    engine.wipe()?;
                }
                Ok(())
            }
            _ => Err(ErrorKind::UnknownEngine(engine.into()).into()),
        }
    }

    pub fn reset(&self, engine: &str) -> Result<()> {
        match engine {
            "bookmarks" | "history" => {
                if let Some(engine) = Self::places_engine(engine) {
                    engine.reset(&EngineSyncAssociation::Disconnected)?;
                }
                Ok(())
            }
            "addresses" | "creditcards" => {
                if let Some(engine) = Self::autofill_engine(engine) {
                    engine.reset(&EngineSyncAssociation::Disconnected)?;
                }
                Ok(())
            }
            "logins" => {
                if let Some(engine) = Self::logins_engine(engine) {
                    engine.reset(&EngineSyncAssociation::Disconnected)?;
                }
                Ok(())
            }
            _ => Err(ErrorKind::UnknownEngine(engine.into()).into()),
        }
    }

    pub fn reset_all(&self) -> Result<()> {
        if let Some(engine) = Self::places_engine("history") {
            engine.reset(&EngineSyncAssociation::Disconnected)?;
        }
        if let Some(engine) = Self::places_engine("bookmarks") {
            engine.reset(&EngineSyncAssociation::Disconnected)?;
        }
        if let Some(addresses) = Self::autofill_engine("addresses") {
            addresses.reset(&EngineSyncAssociation::Disconnected)?;
        }
        if let Some(credit_cards) = Self::autofill_engine("creditcards") {
            credit_cards.reset(&EngineSyncAssociation::Disconnected)?;
        }
        Ok(())
    }

    pub fn disconnect(&self) {
        if let Some(engine) = Self::places_engine("bookmarks") {
            if let Err(e) = engine.reset(&EngineSyncAssociation::Disconnected) {
                log::error!("Failed to reset bookmarks: {}", e);
            }
        } else {
            log::warn!("Unable to reset bookmarks, be sure to call register_with_sync_manager before disconnect if this is surprising");
        }
        if let Some(engine) = Self::places_engine("history") {
            if let Err(e) = engine.reset(&EngineSyncAssociation::Disconnected) {
                log::error!("Failed to reset history: {}", e);
            }
        } else {
            log::warn!("Unable to reset history, be sure to call register_with_sync_manager before disconnect if this is surprising");
        }
        if let Some(addresses) = Self::autofill_engine("addresses") {
            if let Err(e) = addresses.reset(&EngineSyncAssociation::Disconnected) {
                log::error!("Failed to reset addresses: {}", e);
            }
        } else {
            log::warn!("Unable to reset addresses, be sure to call register_with_sync_manager before disconnect if this is surprising");
        }
        if let Some(credit_cards) = Self::autofill_engine("creditcards") {
            if let Err(e) = credit_cards.reset(&EngineSyncAssociation::Disconnected) {
                log::error!("Failed to reset credit cards: {}", e);
            }
        } else {
            log::warn!("Unable to reset credit cards, be sure to call register_with_sync_manager before disconnect if this is surprising");
        }
        if let Some(logins) = Self::logins_engine("logins") {
            if let Err(e) = logins.reset(&EngineSyncAssociation::Disconnected) {
                log::error!("Failed to reset logins: {}", e);
            }
        } else {
            log::warn!("Unable to reset logins, be sure to call register_with_sync_manager before disconnect if this is surprising");
        }
    }

    pub fn sync(&self, params: SyncParams) -> Result<SyncResult> {
        let mut state = self.mem_cached_state.lock().unwrap();
        let mut have_engines = vec![];
        let bookmarks = Self::places_engine("bookmarks");
        let history = Self::places_engine("history");
        let tabs = Self::tabs_engine("tabs");
        let logins = Self::logins_engine("logins");
        let addresses = Self::autofill_engine("addresses");
        let credit_cards = Self::autofill_engine("creditcards");
        if bookmarks.is_some() {
            have_engines.push(BOOKMARKS_ENGINE);
        }
        if history.is_some() {
            have_engines.push(HISTORY_ENGINE);
        }
        if logins.is_some() {
            have_engines.push(LOGINS_ENGINE);
        }
        if tabs.is_some() {
            have_engines.push(TABS_ENGINE);
        }
        if addresses.is_some() {
            have_engines.push(ADDRESSES_ENGINE);
        }
        if credit_cards.is_some() {
            have_engines.push(CREDIT_CARDS_ENGINE);
        }
        check_engine_list(&params.engines_to_sync, &have_engines)?;

        let next_sync_after = state
            .as_ref()
            .and_then(|mcs| mcs.get_next_sync_after());
        if !backoff_in_effect(next_sync_after, &params) {
            log::info!("No backoff in effect (or we decided to ignore it), starting sync");
            self.do_sync(params, &mut state)
        } else {
            let ts = system_time_to_millis(next_sync_after);
            log::warn!(
                "Backoff still in effect (until {:?}), bailing out early",
                ts
            );
            Ok(SyncResult {
                status: ServiceStatus::BackedOff as i32,
                results: Default::default(),
                have_declined: false,
                declined: vec![],
                next_sync_allowed_at: ts,
                persisted_state: params.persisted_state.unwrap_or_default(),
                // It would be nice to record telemetry here.
                telemetry_json: None,
            })
        }
    }

    fn do_sync(&self, mut params: SyncParams, state: &mut Option<MemoryCachedState>) -> Result<SyncResult> {
        let bookmarks = Self::places_engine("bookmarks");
        let history = Self::places_engine("history");
        let tabs = Self::tabs_engine("tabs");
        let logins = Self::logins_engine("logins");
        let addresses = Self::autofill_engine("addresses");
        let credit_cards = Self::autofill_engine("creditcards");

        let key_bundle = sync15::KeyBundle::from_ksync_base64(&params.acct_sync_key)?;
        let tokenserver_url = url::Url::parse(&params.acct_tokenserver_url)?;

        let bookmarks_sync = should_sync(&params, BOOKMARKS_ENGINE) && bookmarks.is_some();
        let history_sync = should_sync(&params, HISTORY_ENGINE) && history.is_some();
        let logins_sync = should_sync(&params, LOGINS_ENGINE) && logins.is_some();
        let tabs_sync = should_sync(&params, TABS_ENGINE) && tabs.is_some();
        let addresses_sync = should_sync(&params, ADDRESSES_ENGINE) && addresses.is_some();
        let credit_cards_sync = should_sync(&params, CREDIT_CARDS_ENGINE) && credit_cards.is_some();

        let bs = if bookmarks_sync { bookmarks } else { None };
        let hs = if history_sync { history } else { None };
        let ts = if tabs_sync { tabs } else { None };
        let ls = if logins_sync { logins } else { None };
        let ads = if addresses_sync { addresses } else { None };
        let cs = if credit_cards_sync {
            credit_cards
        } else {
            None
        };

        // TODO(issue 1684) this isn't ideal, we should have real support for interruption.
        let p = Arc::new(AtomicUsize::new(0));
        let interruptee = sql_support::SqlInterruptScope::new(p);

        let mut mem_cached_state = state.take().unwrap_or_default();
        let mut disk_cached_state = params.persisted_state.take();
        // `sync_multiple` takes a &[&dyn Engine], but we need something to hold
        // ownership of our engines.
        let mut engines: Vec<Box<dyn sync15::SyncEngine>> = vec![];

        if let Some(bookmarks) = bs {
            assert!(bookmarks_sync, "Should have already checked");
            engines.push(bookmarks);
        }

        if let Some(history) = hs {
            assert!(history_sync, "Should have already checked");
            engines.push(history);
        }

        if let Some(logins) = ls {
            assert!(logins_sync, "Should have already checked");
            engines.push(logins);
        }

        if let Some(tbs) = ts {
            assert!(tabs_sync, "Should have already checked");
            engines.push(tbs);
        }

        if let Some(add) = ads {
            assert!(addresses_sync, "Should have already checked");
            engines.push(add);
        }

        if let Some(cc) = cs {
            assert!(credit_cards_sync, "Should have already checked");
            engines.push(cc);
        }

        // tell engines about the local encryption key.
        for engine in engines.iter_mut() {
            if let Some(key) = params.local_encryption_keys.get(&*engine.collection_name()) {
                engine.set_local_encryption_key(key)?
            }
        }

        let engine_refs: Vec<&dyn sync15::SyncEngine> = engines.iter().map(|s| &**s).collect();

        let client_init = sync15::Sync15StorageClientInit {
            key_id: params.acct_key_id.clone(),
            access_token: params.acct_access_token.clone(),
            tokenserver_url,
        };
        let engines_to_change = if params.engines_to_change_state.is_empty() {
            None
        } else {
            Some(&params.engines_to_change_state)
        };

        let settings = Settings {
            fxa_device_id: params.fxa_device_id,
            device_name: params.device_name,
            device_type: match params.device_type {
                DEVICE_TYPE_DESKTOP => clients::DeviceType::Desktop,
                DEVICE_TYPE_MOBILE => clients::DeviceType::Mobile,
                DEVICE_TYPE_TABLET => clients::DeviceType::Tablet,
                DEVICE_TYPE_VR => clients::DeviceType::VR,
                DEVICE_TYPE_TV => clients::DeviceType::TV,
                _ => {
                    log::warn!(
                        "Unknown device type {}; assuming desktop",
                        params.device_type
                    );
                    clients::DeviceType::Desktop
                }
            },
        };
        let c = SyncClient::new(settings);
        let result = sync15::sync_multiple_with_command_processor(
            Some(&c),
            &engine_refs,
            &mut disk_cached_state,
            &mut mem_cached_state,
            &client_init,
            &key_bundle,
            &interruptee,
            Some(sync15::SyncRequestInfo {
                engines_to_state_change: engines_to_change,
                is_user_action: params.reason == (SyncReason::User as i32),
            }),
        );
        *state = Some(mem_cached_state);

        log::info!("Sync finished with status {:?}", result.service_status);
        let status = ServiceStatus::from(result.service_status) as i32;
        let results: HashMap<String, String> = result
            .engine_results
            .into_iter()
            .map(|(e, r)| {
                log::info!("engine {:?} status: {:?}", e, r);
                (
                    e,
                    match r {
                        Ok(()) => "".to_string(),
                        Err(err) => {
                            let msg = err.to_string();
                            if msg.is_empty() {
                                log::error!(
                                    "Bug: error message string is empty for error: {:?}",
                                    err
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
    log::trace!(
        "Checking engines requested ({:?}) vs local engines ({:?})",
        list,
        have_engines
    );
    for e in list {
        if [
            ADDRESSES_ENGINE,
            CREDIT_CARDS_ENGINE,
            BOOKMARKS_ENGINE,
            HISTORY_ENGINE,
            LOGINS_ENGINE,
            TABS_ENGINE,
        ]
        .contains(&e.as_ref())
        {
            if !have_engines.iter().any(|engine| e == engine) {
                return Err(ErrorKind::UnsupportedFeature(e.to_string()).into());
            }
        } else {
            return Err(ErrorKind::UnknownEngine(e.to_string()).into());
        }
    }
    Ok(())
}

struct SyncClient(Settings);

impl SyncClient {
    pub fn new(settings: Settings) -> SyncClient {
        SyncClient(settings)
    }
}

impl CommandProcessor for SyncClient {
    fn settings(&self) -> &Settings {
        &self.0
    }

    fn apply_incoming_command(&self, command: Command) -> anyhow::Result<CommandStatus> {
        let result = match command {
            Command::Wipe(engine) => wipe(&engine),
            Command::Reset(engine) => reset(&engine),
            Command::ResetAll => reset_all(),
        };
        match result {
            Ok(()) => Ok(CommandStatus::Applied),
            Err(err) => match err.kind() {
                ErrorKind::UnknownEngine(_) => Ok(CommandStatus::Unsupported),
                _ => Err(err.into()),
            },
        }
    }

    fn fetch_outgoing_commands(&self) -> anyhow::Result<HashSet<Command>> {
        Ok(HashSet::new())
    }
}
