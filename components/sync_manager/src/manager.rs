/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::types::{ServiceStatus, SyncEngineSelection, SyncParams, SyncReason, SyncResult};
use crate::{reset, reset_all, wipe};
use std::collections::{HashMap, HashSet};
use std::sync::{atomic::AtomicUsize, Arc, Mutex};
use std::time::SystemTime;
use sync15::{
    self,
    clients::{Command, CommandProcessor, CommandStatus, Settings},
    EngineSyncAssociation, MemoryCachedState, SyncEngine,
};

const LOGINS_ENGINE: &str = "passwords";
const HISTORY_ENGINE: &str = "history";
const BOOKMARKS_ENGINE: &str = "bookmarks";
const TABS_ENGINE: &str = "tabs";
const ADDRESSES_ENGINE: &str = "addresses";
const CREDIT_CARDS_ENGINE: &str = "creditcards";

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

    fn get_engine(engine: &str) -> Option<Box<dyn SyncEngine>> {
        match engine {
            "history" => Self::places_engine("history"),
            "bookmarks" => Self::places_engine("bookmarks"),
            "addresses" => Self::autofill_engine("addresses"),
            "creditcards" => Self::autofill_engine("creditcards"),
            "logins" => Self::logins_engine("logins"),
            "tabs" => Self::tabs_engine("tabs"),
            _ => {
                log::warn!("get_engine() unknown engine: {}", engine);
                None
            }
        }
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
            _ => Err(SyncManagerError::UnknownEngine(engine.into())),
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
            _ => Err(SyncManagerError::UnknownEngine(engine.into())),
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

    /// Disconnect engines from sync, deleting/resetting the sync-related data
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

    /// Perform a sync.  See [SyncParams] and [SyncResult] for details on how this works
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
        check_engine_list(&params.engines, &have_engines)?;

        let next_sync_after = state.as_ref().and_then(|mcs| mcs.get_next_sync_after());
        if !backoff_in_effect(next_sync_after, &params) {
            log::info!("No backoff in effect (or we decided to ignore it), starting sync");
            self.do_sync(params, &mut state)
        } else {
            log::warn!(
                "Backoff still in effect (until {:?}), bailing out early",
                next_sync_after
            );
            Ok(SyncResult {
                status: ServiceStatus::BackedOff,
                successful: Default::default(),
                failures: Default::default(),
                declined: None,
                next_sync_allowed_at: next_sync_after,
                persisted_state: params.persisted_state.unwrap_or_default(),
                // It would be nice to record telemetry here.
                telemetry_json: None,
            })
        }
    }

    fn do_sync(
        &self,
        mut params: SyncParams,
        state: &mut Option<MemoryCachedState>,
    ) -> Result<SyncResult> {
        let bookmarks = Self::places_engine("bookmarks");
        let history = Self::places_engine("history");
        let tabs = Self::tabs_engine("tabs");
        let logins = Self::logins_engine("logins");
        let addresses = Self::autofill_engine("addresses");
        let credit_cards = Self::autofill_engine("creditcards");

        let key_bundle = sync15::KeyBundle::from_ksync_base64(&params.auth_info.sync_key)?;
        let tokenserver_url = url::Url::parse(&params.auth_info.tokenserver_url)?;

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
            key_id: params.auth_info.kid.clone(),
            access_token: params.auth_info.fxa_access_token.clone(),
            tokenserver_url,
        };
        let engines_to_change = if params.enabled_changes.is_empty() {
            None
        } else {
            Some(&params.enabled_changes)
        };

        let settings = Settings {
            fxa_device_id: params.device_settings.fxa_device_id,
            device_name: params.device_settings.name,
            device_type: params.device_settings.kind,
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
                is_user_action: matches!(params.reason, SyncReason::User),
            }),
        );
        *state = Some(mem_cached_state);

        log::info!("Sync finished with status {:?}", result.service_status);
        let status = ServiceStatus::from(result.service_status);
        for (engine, result) in result.engine_results.iter() {
            log::info!("engine {:?} status: {:?}", engine, result);
        }
        let mut successful: Vec<String> = Vec::new();
        let mut failures: HashMap<String, String> = HashMap::new();
        for (engine, result) in result.engine_results.into_iter() {
            match result {
                Ok(_) => {
                    successful.push(engine);
                }
                Err(err) => {
                    failures.insert(engine, err.to_string());
                }
            }
        }
        let telemetry_json = serde_json::to_string(&result.telemetry).unwrap();

        Ok(SyncResult {
            status,
            successful,
            failures,
            declined: result.declined,
            next_sync_allowed_at: result.next_sync_after,
            persisted_state: disk_cached_state.unwrap_or_default(),
            telemetry_json: Some(telemetry_json),
        })
    }

    pub fn get_available_engines(&self) -> Vec<String> {
        let engine_names = vec![
            "bookmarks",
            "history",
            "tabs",
            "logins",
            "addresses",
            "creditcards",
        ];
        engine_names
            .into_iter()
            .filter_map(|name| Self::get_engine(name).map(|_| name.to_string()))
            .collect()
    }
}

fn backoff_in_effect(next_sync_after: Option<SystemTime>, p: &SyncParams) -> bool {
    let now = SystemTime::now();
    if let Some(nsa) = next_sync_after {
        if nsa > now {
            return if matches!(p.reason, SyncReason::User | SyncReason::EnabledChange) {
                log::info!(
                    "Still under backoff, but syncing anyway because reason is {:?}",
                    p.reason
                );
                false
            } else if !p.enabled_changes.is_empty() {
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

fn should_sync(p: &SyncParams, engine: &str) -> bool {
    match &p.engines {
        SyncEngineSelection::All => true,
        SyncEngineSelection::Some { engines } => engines.iter().any(|e| e == engine),
    }
}

fn check_engine_list(selection: &SyncEngineSelection, have_engines: &[&str]) -> Result<()> {
    log::trace!(
        "Checking engines requested ({:?}) vs local engines ({:?})",
        selection,
        have_engines
    );
    match selection {
        SyncEngineSelection::All => Ok(()),
        SyncEngineSelection::Some { engines } => {
            for e in engines {
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
                        return Err(SyncManagerError::UnsupportedFeature(e.to_string()));
                    }
                } else {
                    return Err(SyncManagerError::UnknownEngine(e.to_string()));
                }
            }
            Ok(())
        }
    }
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
            Err(err) => match err {
                SyncManagerError::UnknownEngine(_) => Ok(CommandStatus::Unsupported),
                _ => Err(err.into()),
            },
        }
    }

    fn fetch_outgoing_commands(&self) -> anyhow::Result<HashSet<Command>> {
        Ok(HashSet::new())
    }
}
