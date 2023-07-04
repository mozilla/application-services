/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, HashSet};

use crate::{
    internal::{
        commands, device,
        oauth::{AccessTokenInfo, RefreshToken},
        profile::Profile,
        state_persistence::state_to_json,
        CachedResponse, Config, OAuthFlow, PersistedState,
    },
    Result, ScopedKey,
};

/// Stores and manages the current state of the FxA client
///
/// All fields are private, which means that all state mutations must go through this module.  This
/// makes it easier to reason about state changes.
pub struct StateManager {
    /// State that's persisted to disk
    persisted_state: PersistedState,
    /// In-progress OAuth flows
    flow_store: HashMap<String, OAuthFlow>,
}

impl StateManager {
    pub(crate) fn new(persisted_state: PersistedState) -> Self {
        Self {
            persisted_state,
            flow_store: HashMap::new(),
        }
    }

    pub fn serialize_persisted_state(&self) -> Result<String> {
        state_to_json(&self.persisted_state)
    }

    pub fn config(&self) -> &Config {
        &self.persisted_state.config
    }

    pub fn refresh_token(&self) -> Option<&RefreshToken> {
        self.persisted_state.refresh_token.as_ref()
    }

    pub fn session_token(&self) -> Option<&str> {
        self.persisted_state.session_token.as_deref()
    }

    /// Get the last known set of device capabilities that we sent to the server
    pub fn last_sent_device_capabilities(&self) -> &HashSet<device::Capability> {
        &self.persisted_state.device_capabilities
    }

    /// Update the last known set of device capabilities that we sent to the server
    pub fn update_last_sent_device_capabilities(
        &mut self,
        capabilities_set: HashSet<device::Capability>,
    ) {
        self.persisted_state.device_capabilities = capabilities_set;
    }

    /// Clear out the last known set of device_capabilities.  This means that the next call to
    /// `ensure_capabilities()` will re-send our capabilities to the server
    ///
    /// This is typically called when something may invalidate the server's knowledge of our
    /// capabilities, for example replacing our device info.
    pub fn clear_last_sent_device_capabilities(&mut self) {
        self.persisted_state.device_capabilities = HashSet::new();
    }

    pub fn send_tab_key(&self) -> Option<&str> {
        self.persisted_state
            .commands_data
            .get(commands::send_tab::COMMAND_NAME)
            .map(String::as_str)
    }

    pub fn set_send_tab_key(&mut self, key: String) {
        self.persisted_state
            .commands_data
            .insert(commands::send_tab::COMMAND_NAME.into(), key);
    }

    pub fn clear_send_tab_key(&mut self) {
        self.persisted_state
            .commands_data
            .remove(commands::send_tab::COMMAND_NAME);
    }

    pub fn last_handled_command_index(&self) -> Option<u64> {
        self.persisted_state.last_handled_command
    }

    pub fn set_last_handled_command_index(&mut self, idx: u64) {
        self.persisted_state.last_handled_command = Some(idx)
    }

    pub fn current_device_id(&self) -> Option<&str> {
        self.persisted_state.current_device_id.as_deref()
    }

    pub fn set_current_device_id(&mut self, device_id: String) {
        self.persisted_state.current_device_id = Some(device_id);
    }

    pub fn get_scoped_key(&self, scope: &str) -> Option<&ScopedKey> {
        self.persisted_state.scoped_keys.get(scope)
    }

    pub(crate) fn last_seen_profile(&self) -> Option<&CachedResponse<Profile>> {
        self.persisted_state.last_seen_profile.as_ref()
    }

    pub(crate) fn set_last_seen_profile(&mut self, profile: CachedResponse<Profile>) {
        self.persisted_state.last_seen_profile = Some(profile)
    }

    pub fn clear_last_seen_profile(&mut self) {
        self.persisted_state.last_seen_profile = None
    }

    pub fn get_cached_access_token(&mut self, scope: &str) -> Option<&AccessTokenInfo> {
        self.persisted_state.access_token_cache.get(scope)
    }

    pub fn add_cached_access_token(&mut self, scope: impl Into<String>, token: AccessTokenInfo) {
        self.persisted_state
            .access_token_cache
            .insert(scope.into(), token);
    }

    pub fn clear_access_token_cache(&mut self) {
        self.persisted_state.access_token_cache.clear()
    }

    /// Begin an OAuth flow.  This saves the OAuthFlow for later.  `state` must be unique to this
    /// oauth flow process.
    pub fn begin_oauth_flow(&mut self, state: impl Into<String>, flow: OAuthFlow) {
        self.flow_store.insert(state.into(), flow);
    }

    /// Get an OAuthFlow from a previous `begin_oauth_flow()` call
    ///
    /// This operation removes the OAuthFlow from the our internal map.  It can only be called once
    /// per `state` value.
    pub fn pop_oauth_flow(&mut self, state: &str) -> Option<OAuthFlow> {
        self.flow_store.remove(state)
    }

    /// Complete an OAuth flow.
    pub fn complete_oauth_flow(
        &mut self,
        scoped_keys: Vec<(String, ScopedKey)>,
        refresh_token: RefreshToken,
        session_token: Option<String>,
    ) {
        // When our keys change, we might need to re-register device capabilities with the server.
        // Ensure that this happens on the next call to ensure_capabilities.
        self.persisted_state.device_capabilities.clear();

        for (scope, key) in scoped_keys {
            self.persisted_state.scoped_keys.insert(scope, key);
        }
        self.persisted_state.refresh_token = Some(refresh_token);
        self.persisted_state.session_token = session_token;
        self.flow_store.clear();
    }

    /// Called when the account is disconnected.  This clears most of the auth state, but keeps
    /// some information in order to eventually reconnect to the same user account later.
    pub fn disconnect(&mut self) {
        self.persisted_state = self.persisted_state.start_over();
        self.flow_store.clear();
    }

    /// Handle the auth tokens changing
    ///
    /// This method updates the token data and clears out data that may be invalidated with the
    /// token changes.
    pub fn update_tokens(&mut self, session_token: String, refresh_token: RefreshToken) {
        self.persisted_state.session_token = Some(session_token);
        self.persisted_state.refresh_token = Some(refresh_token);
        self.persisted_state.access_token_cache.clear();
        self.persisted_state.device_capabilities.clear();
    }
}

#[cfg(test)]
impl StateManager {
    pub fn is_access_token_cache_empty(&self) -> bool {
        self.persisted_state.access_token_cache.is_empty()
    }

    pub fn force_refresh_token(&mut self, token: RefreshToken) {
        self.persisted_state.refresh_token = Some(token)
    }

    pub fn force_session_token(&mut self, token: String) {
        self.persisted_state.session_token = Some(token)
    }

    pub fn force_current_device_id(&mut self, device_id: impl Into<String>) {
        self.persisted_state.current_device_id = Some(device_id.into())
    }

    pub fn insert_scoped_key(&mut self, scope: impl Into<String>, key: ScopedKey) {
        self.persisted_state.scoped_keys.insert(scope.into(), key);
    }
}
