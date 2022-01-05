/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::types::{ServiceStatus, SyncEngineSelection, SyncParams, SyncReason, SyncResult};
use crate::{reset, reset_all, wipe};
use interrupt_support::InterruptScope;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::time::SystemTime;
use sync15::{
    self,
    clients::{Command, CommandProcessor, CommandStatus, Settings},
    EngineSyncAssociation, MemoryCachedState, SyncEngine, SyncEngineId,
};

#[derive(Default)]
pub struct SyncManager {
    mem_cached_state: Mutex<Option<MemoryCachedState>>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_engine_id(engine_name: &str) -> Result<SyncEngineId> {
        SyncEngineId::try_from(engine_name).map_err(SyncManagerError::UnknownEngine)
    }

    // Interrupt any active sync operations
    pub fn interrupt(&self) {
        InterruptScope::interrupt();
        places::interrupt_current_sync_query();
    }

    fn get_engine(
        engine_id: &SyncEngineId,
        interrupt_scope: InterruptScope,
    ) -> Option<Box<dyn SyncEngine>> {
        match engine_id {
            SyncEngineId::History => places::get_registered_sync_engine(engine_id, interrupt_scope),
            SyncEngineId::Bookmarks => {
                places::get_registered_sync_engine(engine_id, interrupt_scope)
            }
            SyncEngineId::Addresses => {
                autofill::get_registered_sync_engine(engine_id, interrupt_scope)
            }
            SyncEngineId::CreditCards => {
                autofill::get_registered_sync_engine(engine_id, interrupt_scope)
            }
            SyncEngineId::Passwords => {
                logins::get_registered_sync_engine(engine_id, interrupt_scope)
            }
            SyncEngineId::Tabs => tabs::get_registered_sync_engine(engine_id, interrupt_scope),
        }
    }

    fn get_engine_from_name(
        engine_name: &str,
        interrupt_scope: InterruptScope,
    ) -> Result<Option<Box<dyn SyncEngine>>> {
        Ok(Self::get_engine(
            &Self::get_engine_id(engine_name)?,
            interrupt_scope,
        ))
    }

    pub fn wipe(&self, engine_name: &str) -> Result<()> {
        if let Some(engine) = Self::get_engine_from_name(engine_name, InterruptScope::new())? {
            engine.wipe()?;
        }
        Ok(())
    }

    pub fn reset(&self, engine_name: &str) -> Result<()> {
        if let Some(engine) = Self::get_engine_from_name(engine_name, InterruptScope::new())? {
            engine.reset(&EngineSyncAssociation::Disconnected)?;
        }
        Ok(())
    }

    pub fn reset_all(&self) -> Result<()> {
        let interrupt_scope = InterruptScope::new();
        for (_, engine) in self.iter_registered_engines(&interrupt_scope) {
            engine.reset(&EngineSyncAssociation::Disconnected)?;
        }
        Ok(())
    }

    /// Disconnect engines from sync, deleting/resetting the sync-related data
    pub fn disconnect(&self) {
        let interrupt_scope = InterruptScope::new();
        for engine_id in SyncEngineId::iter() {
            if let Some(engine) = Self::get_engine(&engine_id, interrupt_scope.clone()) {
                if let Err(e) = engine.reset(&EngineSyncAssociation::Disconnected) {
                    log::error!("Failed to reset {}: {}", engine_id, e);
                }
            } else {
                log::warn!("Unable to reset {}, be sure to call register_with_sync_manager before disconnect if this is surprising", engine_id);
            }
        }
    }

    /// Perform a sync.  See [SyncParams] and [SyncResult] for details on how this works
    pub fn sync(&self, params: SyncParams) -> Result<SyncResult> {
        let interrupt_scope = InterruptScope::new();
        let mut state = self.mem_cached_state.lock();
        let engines = self.calc_engines_to_sync(&params.engines, &interrupt_scope)?;
        let next_sync_after = state.as_ref().and_then(|mcs| mcs.get_next_sync_after());
        if !backoff_in_effect(next_sync_after, &params) {
            log::info!("No backoff in effect (or we decided to ignore it), starting sync");
            self.do_sync(params, &mut state, engines)
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
        mut engines: Vec<Box<dyn SyncEngine>>,
    ) -> Result<SyncResult> {
        let key_bundle = sync15::KeyBundle::from_ksync_base64(&params.auth_info.sync_key)?;
        let tokenserver_url = url::Url::parse(&params.auth_info.tokenserver_url)?;
        let interrupt_scope = InterruptScope::new();
        let mut mem_cached_state = state.take().unwrap_or_default();
        let mut disk_cached_state = params.persisted_state.take();

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
            &interrupt_scope,
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

    fn iter_registered_engines<'a>(
        &'a self,
        interrupt_scope: &'a InterruptScope,
    ) -> impl Iterator<Item = (SyncEngineId, Box<dyn SyncEngine>)> + 'a {
        SyncEngineId::iter().filter_map(move |id| {
            Self::get_engine(&id, interrupt_scope.clone()).map(|engine| (id, engine))
        })
    }

    pub fn get_available_engines(&self) -> Vec<String> {
        let interrupt_scope = InterruptScope::new();
        self.iter_registered_engines(&interrupt_scope)
            .map(|(name, _)| name.to_string())
            .collect()
    }

    fn calc_engines_to_sync(
        &self,
        selection: &SyncEngineSelection,
        interrupt_scope: &InterruptScope,
    ) -> Result<Vec<Box<dyn SyncEngine>>> {
        let mut engine_map: HashMap<_, _> = self.iter_registered_engines(interrupt_scope).collect();
        log::trace!(
            "Checking engines requested ({:?}) vs local engines ({:?})",
            selection,
            engine_map
                .iter()
                .map(|(engine_id, _)| engine_id.name())
                .collect::<Vec<_>>(),
        );
        if let SyncEngineSelection::Some {
            engines: engine_names,
        } = selection
        {
            // Validate selection and convert to SyncEngineId
            let mut selected_engine_ids: HashSet<SyncEngineId> = HashSet::new();
            for name in engine_names {
                let engine_id = Self::get_engine_id(name)?;
                if !engine_map.contains_key(&engine_id) {
                    return Err(SyncManagerError::UnsupportedFeature(name.to_string()));
                }
                selected_engine_ids.insert(engine_id);
            }
            // Filter engines based on the selection
            engine_map = engine_map
                .into_iter()
                .filter(|(engine_id, _)| selected_engine_ids.contains(engine_id))
                .collect()
        }
        Ok(engine_map.into_iter().map(|(_, engine)| engine).collect())
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_engine_id_sanity() {
        for engine_id in SyncEngineId::iter() {
            assert_eq!(engine_id, SyncEngineId::try_from(engine_id.name()).unwrap());
        }
    }
}
