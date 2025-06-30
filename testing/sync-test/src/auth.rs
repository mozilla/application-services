/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

use anyhow::Result;
use autofill::db::store::Store as AutofillStore;
use cli_support::fxa_creds::CliFxa;
use fxa_client::{Device, FxaConfig, FxaServer};
use logins::encryption::{
    create_key, EncryptorDecryptor, ManagedEncryptorDecryptor, StaticKeyManager,
};
use logins::LoginStore;
use std::collections::{hash_map::RandomState, HashMap};
use std::sync::Arc;
use sync15::{
    client::{SetupStorageClient, Sync15StorageClient},
    DeviceType,
};
use sync_manager::{
    manager::SyncManager, DeviceSettings, SyncEngineSelection, SyncParams, SyncReason,
};
use tabs::TabsStore;

// Note that in revision ba9cc53710243c357e5bf125636bb94a7c6a4a48 or so, this had support for
// username/password authentication and for "temporary" accounts using restmail etc.
// It has since been refactored to use only oauth and to use the "cli-support" crate to help
// manage the account, but you can use that revision if you ever want to revive username/password
// support.

// Many of these tests simulate multiple devices (aka clients).
pub struct TestClient {
    pub cli: Arc<CliFxa>,
    pub device: Device,
    // XXX do this more generically...
    pub autofill_store: Arc<AutofillStore>,
    pub logins_store: Arc<LoginStore>,
    pub encdec: Arc<dyn EncryptorDecryptor>,
    pub tabs_store: Arc<TabsStore>,
    sync_manager: SyncManager,
    persisted_state: Option<String>,
}

impl TestClient {
    pub fn new(cli: Arc<CliFxa>, device_name: &str) -> Result<Self> {
        // XXX - not clear if/how this device gets cleaned up - we never disconnect from the account!
        // And this is messy - I think it reflects that the public device api should be improved?
        let device = match cli
            .account
            .get_devices(false)?
            .into_iter()
            .find(|d| d.is_current_device)
        {
            Some(d) => d,
            None => {
                cli.account
                    .initialize_device(device_name, DeviceType::Desktop, vec![])?;
                cli.account
                    .get_devices(true)?
                    .into_iter()
                    .find(|d| d.is_current_device)
                    .ok_or_else(|| anyhow::Error::msg("can't find new device"))?
            }
        };

        let key = create_key().unwrap();
        let encdec = Arc::new(ManagedEncryptorDecryptor::new(Arc::new(
            StaticKeyManager::new(key.clone()),
        )));

        Ok(Self {
            cli,
            device,
            autofill_store: Arc::new(AutofillStore::new_shared_memory("sync-test")?),
            logins_store: Arc::new(LoginStore::new(":memory:", encdec.clone())?),
            encdec,
            tabs_store: Arc::new(TabsStore::new_with_mem_path("sync-test-tabs")),
            sync_manager: SyncManager::new(),
            persisted_state: None,
        })
    }

    pub fn sync(
        &mut self,
        engines: &[String],
        local_encryption_keys: HashMap<String, String>,
    ) -> Result<()> {
        // ensure all our engines are registered.
        self.autofill_store.clone().register_with_sync_manager();
        self.tabs_store.clone().register_with_sync_manager();
        self.logins_store.clone().register_with_sync_manager();
        let params = SyncParams {
            reason: SyncReason::User,
            engines: SyncEngineSelection::Some {
                engines: engines.to_vec(),
            },
            enabled_changes: HashMap::new(),
            local_encryption_keys,
            auth_info: self.cli.as_auth_info(),
            persisted_state: self.persisted_state.take(),
            device_settings: DeviceSettings {
                fxa_device_id: self.device.id.clone(),
                name: self.device.display_name.clone(),
                kind: self.device.device_type,
            },
        };
        let result = self.sync_manager.sync(params)?;
        // We expect all syncs in these tests to pass, so let's catch that here
        // rather than waiting for a test to fail later.
        assert!(
            result.status.is_ok(),
            "Service status is not OK: {:?}",
            result.status
        );
        assert!(
            result.failures.is_empty(),
            "Engines failed: {:?}",
            result.failures
        );
        self.persisted_state = Some(result.persisted_state);
        Ok(())
    }

    pub fn sync_with_failure(
        &mut self,
        engines: &[String],
        local_encryption_keys: HashMap<String, String>,
    ) -> Result<HashMap<String, String, RandomState>> {
        // ensure all our engines are registered.
        self.autofill_store.clone().register_with_sync_manager();
        self.tabs_store.clone().register_with_sync_manager();
        self.logins_store.clone().register_with_sync_manager();
        let params = SyncParams {
            reason: SyncReason::User,
            engines: SyncEngineSelection::Some {
                engines: engines.to_vec(),
            },
            enabled_changes: HashMap::new(),
            local_encryption_keys,
            auth_info: self.cli.as_auth_info(),
            persisted_state: self.persisted_state.take(),
            device_settings: DeviceSettings {
                fxa_device_id: self.device.id.clone(),
                name: self.device.display_name.clone(),
                kind: self.device.device_type,
            },
        };
        let result = self.sync_manager.sync(params)?;
        // Syncs initiated with this function should fail otherwise `sync` should be used.
        assert!(
            result.status.is_ok(),
            "Service status is not OK: {:?}",
            result.status
        );
        assert!(!result.failures.is_empty(), "No engine failures");
        self.persisted_state = Some(result.persisted_state);
        Ok(result.failures)
    }

    pub fn fully_wipe_server(&mut self) -> Result<()> {
        Sync15StorageClient::new(self.cli.client_init.clone())?.wipe_all_remote()?;
        Ok(())
    }

    pub fn fully_reset_local_db(&mut self) -> Result<()> {
        // Not great...
        self.autofill_store = Arc::new(AutofillStore::new_shared_memory("sync-test")?);
        self.logins_store = Arc::new(LoginStore::new(":memory:", self.encdec.clone())?);
        self.tabs_store = Arc::new(TabsStore::new_with_mem_path("sync-test-tabs"));
        Ok(())
    }
}

// Wipes the server using the first client that can manage it.
// We do this at the end of each test to avoid creating N accounts for N tests,
// and just creating 1 account per file containing tests.
// TODO: this probably shouldn't take a vec but whatever.
pub fn cleanup_server(clients: Vec<&mut TestClient>) -> Result<()> {
    log::info!("Cleaning up server after tests...");
    for c in clients {
        match c.fully_wipe_server() {
            Ok(()) => return Ok(()),
            Err(e) => {
                log::warn!("Error when wiping server: {:?}", e);
                // and I guess we try again, even though there's no reason
                // the next client should succeed here.
            }
        }
    }
    anyhow::bail!("None of the clients managed to wipe the server!");
}

pub struct TestUser {
    pub clients: Vec<TestClient>,
}

impl TestUser {
    pub fn new(cli: Arc<CliFxa>, client_count: usize) -> Result<Self> {
        let clients = (0..client_count)
            .map(|client_num| {
                let name = format!("Testing Device {client_num}");
                TestClient::new(cli.clone(), &name)
            })
            .collect::<Result<_>>()?;
        Ok(Self { clients })
    }
}

// Should move this into the cli helper?
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum FxaConfigUrl {
    StableDev,
    Stage,
    Release,
    Custom(url::Url),
}

impl FxaConfigUrl {
    pub fn to_config(&self, client_id: &str, redirect: &str) -> FxaConfig {
        match self {
            FxaConfigUrl::StableDev => FxaConfig::stable(client_id, redirect),
            FxaConfigUrl::Stage => FxaConfig::stage(client_id, redirect),
            FxaConfigUrl::Release => FxaConfig::release(client_id, redirect),
            FxaConfigUrl::Custom(url) => FxaConfig {
                server: FxaServer::Custom {
                    url: url.to_string(),
                },
                client_id: client_id.to_string(),
                redirect_uri: redirect.to_string(),
                token_server_url_override: None,
            },
        }
    }
}

// Required for arg parsing
impl std::str::FromStr for FxaConfigUrl {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "release" => FxaConfigUrl::Release,
            "stage" => FxaConfigUrl::Stage,
            "stable-dev" => FxaConfigUrl::StableDev,
            s if s.contains(':') => FxaConfigUrl::Custom(url::Url::parse(s)?),
            _ => {
                anyhow::bail!(
                    "Illegal fxa-stack option '{}', not a url nor a known alias",
                    s
                );
            }
        })
    }
}
