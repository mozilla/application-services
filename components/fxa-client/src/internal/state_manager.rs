/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, HashSet};

use crate::{
    internal::{
        oauth::{AccessTokenInfo, RefreshToken},
        profile::Profile,
        state_persistence::state_to_json,
        CachedResponse, Config, OAuthFlow, PersistedState,
    },
    DeviceCapability, FxaRustAuthState, LocalDevice, Result, ScopedKey,
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

    /// Get our device capabilities
    ///
    /// This is the last set of capabilities passed to `initialize_device` or `ensure_capabilities`
    pub fn device_capabilities(&self) -> &HashSet<DeviceCapability> {
        &self.persisted_state.device_capabilities
    }

    /// Set our device capabilities
    pub fn set_device_capabilities(
        &mut self,
        capabilities_set: impl IntoIterator<Item = DeviceCapability>,
    ) {
        self.persisted_state.device_capabilities = HashSet::from_iter(capabilities_set);
    }

    /// Get the last known LocalDevice info sent back from the server
    pub fn server_local_device_info(&self) -> Option<&LocalDevice> {
        self.persisted_state.server_local_device_info.as_ref()
    }

    /// Update the last known LocalDevice info when getting one back from the server
    pub fn update_server_local_device_info(&mut self, local_device: LocalDevice) {
        self.persisted_state.server_local_device_info = Some(local_device)
    }

    /// Clear out the last known LocalDevice info. This means that the next call to
    /// `ensure_capabilities()` will re-send our capabilities to the server
    ///
    /// This is typically called when something may invalidate the server's knowledge of our
    /// local device capabilities, for example replacing our device info.
    pub fn clear_server_local_device_info(&mut self) {
        self.persisted_state.server_local_device_info = None
    }

    pub fn get_commands_data(&self, key: &str) -> Option<&str> {
        self.persisted_state
            .commands_data
            .get(key)
            .map(String::as_str)
    }

    pub fn set_commands_data(&mut self, key: &str, data: String) {
        self.persisted_state
            .commands_data
            .insert(key.to_string(), data);
    }

    pub fn clear_commands_data(&mut self, key: &str) {
        self.persisted_state.commands_data.remove(key);
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
        new_session_token: Option<String>,
    ) {
        // When our keys change, we might need to re-register device capabilities with the server.
        // Ensure that this happens on the next call to ensure_capabilities.
        self.clear_server_local_device_info();

        for (scope, key) in scoped_keys {
            self.persisted_state.scoped_keys.insert(scope, key);
        }
        self.persisted_state.refresh_token = Some(refresh_token);
        // We prioritize the existing session token if we already have one, because we might have
        // acquired a session token before the oauth flow
        if let (None, Some(new_session_token)) = (self.session_token(), new_session_token) {
            self.set_session_token(new_session_token)
        }
        self.persisted_state.logged_out_from_auth_issues = false;
        self.flow_store.clear();
    }

    /// Called when the account is disconnected.  This clears most of the auth state, but keeps
    /// some information in order to eventually reconnect to the same user account later.
    pub fn disconnect(&mut self) {
        self.persisted_state.current_device_id = None;
        self.persisted_state.refresh_token = None;
        self.persisted_state.scoped_keys = HashMap::new();
        self.persisted_state.last_handled_command = None;
        self.persisted_state.commands_data = HashMap::new();
        self.persisted_state.access_token_cache = HashMap::new();
        self.persisted_state.device_capabilities = HashSet::new();
        self.persisted_state.server_local_device_info = None;
        self.persisted_state.session_token = None;
        self.persisted_state.logged_out_from_auth_issues = false;
        self.flow_store.clear();
    }

    /// Called when we notice authentication issues with the account state.
    ///
    /// This clears the auth state, but leaves some fields untouched. That way, if the user
    /// re-authenticates they can continue using the account without unexpected behavior.  The
    /// fields that don't change compared to `disconnect()` are:
    ///
    ///   * `current_device_id`
    ///   * `device_capabilities`
    ///   * `last_handled_command`
    pub fn on_auth_issues(&mut self) {
        self.persisted_state.refresh_token = None;
        self.persisted_state.scoped_keys = HashMap::new();
        self.persisted_state.commands_data = HashMap::new();
        self.persisted_state.access_token_cache = HashMap::new();
        self.persisted_state.server_local_device_info = None;
        self.persisted_state.session_token = None;
        self.persisted_state.logged_out_from_auth_issues = true;
        self.flow_store.clear();
    }

    /// Called when we begin an OAuth flow.
    ///
    /// This clears out tokens/keys set from the previous time we completed an oauth flow.  In
    /// particular, it clears the session token to avoid
    /// https://bugzilla.mozilla.org/show_bug.cgi?id=1887071.
    pub fn on_begin_oauth(&mut self) {
        self.persisted_state.refresh_token = None;
        self.persisted_state.scoped_keys = HashMap::new();
        self.persisted_state.commands_data = HashMap::new();
        self.persisted_state.access_token_cache = HashMap::new();
        self.persisted_state.session_token = None;
    }

    pub fn get_auth_state(&self) -> FxaRustAuthState {
        if self.persisted_state.refresh_token.is_some() {
            FxaRustAuthState::Connected
        } else if self.persisted_state.logged_out_from_auth_issues {
            FxaRustAuthState::AuthIssues
        } else {
            FxaRustAuthState::Disconnected
        }
    }

    /// Handle the auth tokens changing
    ///
    /// This method updates the token data and clears out data that may be invalidated with the
    /// token changes.
    pub fn update_tokens(&mut self, session_token: String, refresh_token: RefreshToken) {
        self.persisted_state.session_token = Some(session_token);
        self.persisted_state.refresh_token = Some(refresh_token);
        self.persisted_state.access_token_cache.clear();
        self.persisted_state.server_local_device_info = None;
    }

    /// Used by the application to test auth token issues
    pub fn simulate_temporary_auth_token_issue(&mut self) {
        for (_, access_token) in self.persisted_state.access_token_cache.iter_mut() {
            "invalid-data".clone_into(&mut access_token.token)
        }
    }

    /// Used by the application to test auth token issues
    pub fn simulate_permanent_auth_token_issue(&mut self) {
        self.persisted_state.session_token = None;
        self.persisted_state.refresh_token = None;
        self.persisted_state.access_token_cache.clear();
    }
    pub fn set_session_token(&mut self, token: String) {
        self.persisted_state.session_token = Some(token)
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

    pub fn force_current_device_id(&mut self, device_id: impl Into<String>) {
        self.persisted_state.current_device_id = Some(device_id.into())
    }

    pub fn insert_scoped_key(&mut self, scope: impl Into<String>, key: ScopedKey) {
        self.persisted_state.scoped_keys.insert(scope.into(), key);
    }
}
