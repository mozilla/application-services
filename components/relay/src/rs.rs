/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Remote Settings integration for Firefox Relay
//!
//! This module handles fetching allowlist and denylist data from Remote Settings
//! to determine which sites should show Relay email mask suggestions.
//!
//! ## Public Suffix List (PSL) Usage
//!
//! Mobile clients should use PSL libraries to extract domains before calling this API:
//! - **URL**: `https://mail.google.com/inbox` (full URL)
//! - **Host**: `mail.google.com` (hostname from URL)
//! - **Domain**: `google.com` (registrable domain via PSL, also known as eTLD+1)
//!
//! The PSL ensures proper domain matching:
//! - `mail.google.com` → domain `google.com` ✓
//! - `evil-google.com` → domain `evil-google.com` ✗ (won't match rules for google.com)
//! - `mysite.github.io` → domain `mysite.github.io` ✓ (github.io is in PSL)
//!
//! ## Caching Behavior
//!
//! Remote Settings automatically caches collection data in SQLite:
//! - **First call**: Fetches from server and caches (network request)
//! - **Subsequent calls**: Reads from cache (fast, no network)
//! - **Updates**: Mobile apps should call `RemoteSettingsService.sync()` periodically
//!   to refresh all collections, or individual clients will sync on first access
//!
//! This makes the component self-sufficient - it will automatically fetch data when needed
//! and cache it for fast subsequent access.

use crate::error::Result;
use remote_settings::{RemoteSettingsClient, RemoteSettingsService};
use std::sync::Arc;

/// Remote Settings collection names for Relay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Collection {
    Allowlist,
    Denylist,
}

impl Collection {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Allowlist => "fxrelay-allowlist",
            Self::Denylist => "fxrelay-denylist",
        }
    }
}

/// Client for fetching Relay data from Remote Settings
///
/// This struct holds separate Remote Settings clients for the allowlist and denylist
/// collections. It follows the same pattern as SuggestRemoteSettingsClient.
#[derive(Clone, uniffi::Object)]
pub struct RelayRemoteSettingsClient {
    allowlist_client: Arc<RemoteSettingsClient>,
    denylist_client: Arc<RemoteSettingsClient>,
}

#[uniffi::export]
impl RelayRemoteSettingsClient {
    /// Creates a new RelayRemoteSettingsClient from a RemoteSettingsService
    #[uniffi::constructor]
    pub fn new(rs_service: Arc<RemoteSettingsService>) -> Self {
        Self {
            allowlist_client: rs_service.make_client(Collection::Allowlist.name().to_owned()),
            denylist_client: rs_service.make_client(Collection::Denylist.name().to_owned()),
        }
    }

    /// Determines whether Firefox Relay should be offered for a given site.
    ///
    /// This function implements the Relay visibility decision logic based on three factors:
    /// 1. Whether the site is on the denylist
    /// 2. Whether the user is an existing Relay user
    /// 3. Whether the site is on the allowlist
    ///
    /// # Decision Logic (in order)
    ///
    /// 1. If the site is on the **denylist**, don't show Relay (regardless of user status)
    /// 2. If the site is NOT on the denylist and the user **is** a Relay user, show Relay
    /// 3. If the site is NOT on the denylist and the user is **not** a Relay user, check the allowlist:
    ///    - If the site is on the **allowlist**, show Relay (to promote Relay to new users)
    ///    - Otherwise, don't show Relay
    ///
    /// # Fail-Safe Behavior
    ///
    /// If Remote Settings is unavailable:
    /// - If denylist fetch fails: don't show Relay (conservative: avoid showing on potentially blocked sites)
    /// - If allowlist fetch fails for non-Relay users: assume empty (conservative: don't promote without data)
    ///
    /// # Arguments
    /// * `host` - The full hostname (e.g., "mail.google.com", "www.example.com")
    /// * `domain` - The registrable domain from PSL (e.g., "google.com", "example.co.uk")
    /// * `is_relay_user` - Whether the user already has a Relay account
    ///
    /// # Public Suffix List (PSL)
    ///
    /// Mobile clients should use PSL to extract the domain:
    /// - **Host**: Full hostname like "mail.google.com"
    /// - **Domain**: Registrable domain (eTLD+1) like "google.com"
    /// - Swift: Use `URL.host` and extract domain via PSL library
    /// - Kotlin: Use `URL.host` and extract domain via PSL library
    ///
    /// # Returns
    /// `true` if Relay should be shown for this site, `false` otherwise
    ///
    /// # Determining if User is a Relay User
    ///
    /// Mobile clients should check if the user is a Relay user by calling
    /// `FirefoxAccount.getAttachedClients()` and checking if Relay is in the
    /// list of attached clients.
    ///
    /// # Example
    /// ```kotlin
    /// val url = URL("https://mail.google.com/inbox")
    /// val host = url.host // "mail.google.com"
    /// val domain = PublicSuffixList.getRegistrableDomain(host) // "google.com"
    ///
    /// val rsClient = RelayRemoteSettingsClient(remoteSettingsService)
    /// val attachedClients = fxAccount.getAttachedClients()
    /// val isRelayUser = attachedClients.any { it.clientId == RELAY_CLIENT_ID }
    ///
    /// if (rsClient.shouldShowRelay(host, domain, isRelayUser)) {
    ///     // Show Relay UI
    /// }
    /// ```
    pub fn should_show_relay(&self, host: String, domain: String, is_relay_user: bool) -> bool {
        // Validate inputs
        if host.is_empty() || domain.is_empty() {
            log::debug!("Empty host or domain, not showing Relay");
            return false;
        }

        // Log the site and user status we're checking
        log::debug!(
            "Checking if Relay should be shown for host: {}, domain: {}, is_relay_user: {}",
            host,
            domain,
            is_relay_user
        );

        // Fetch denylist with error handling
        let denylist = match self.get_records(Collection::Denylist) {
            Ok(list) => list,
            Err(e) => {
                log::warn!("Failed to fetch denylist, failing conservatively: {}", e);
                // Fail-safe: if we can't fetch denylist, don't show Relay
                // (conservative default - better to not show Relay than risk showing on blocked sites)
                return false;
            }
        };

        // Step 1: Check denylist - if site is denied, never show Relay
        if !denylist.is_empty() && self.is_site_in_list(&denylist, &host, &domain) {
            log::debug!(
                "Site {} ({}) is in denylist, not showing Relay",
                host,
                domain
            );
            return false;
        }

        // Step 2: If site is not on denylist and user is already a Relay user, show Relay
        if is_relay_user {
            log::debug!(
                "Site {} ({}) is not in denylist and user is a Relay user, showing Relay",
                host,
                domain
            );
            return true;
        }

        // Step 3: User is not a Relay user, check allowlist to determine if we should promote Relay
        let allowlist = match self.get_records(Collection::Allowlist) {
            Ok(list) => list,
            Err(e) => {
                log::warn!("Failed to fetch allowlist, assuming empty: {}", e);
                // Fail-safe: if we can't fetch allowlist, don't promote to new users
                // (conservative default - don't show Relay promotion without data)
                vec![]
            }
        };

        // If allowlist is empty, don't show Relay promotion
        if allowlist.is_empty() {
            log::debug!(
                "Allowlist is empty, not showing Relay promotion for non-Relay user on site {} ({})",
                host,
                domain
            );
            return false;
        }

        // Check if site is in allowlist
        let in_allowlist = self.is_site_in_list(&allowlist, &host, &domain);
        if in_allowlist {
            log::debug!(
                "Site {} ({}) is in allowlist, showing Relay promotion to non-Relay user",
                host,
                domain
            );
        } else {
            log::debug!(
                "Site {} ({}) is not in allowlist, not showing Relay promotion to non-Relay user",
                host,
                domain
            );
        }
        in_allowlist
    }
}

impl RelayRemoteSettingsClient {
    /// Gets the Remote Settings client for a specific collection
    fn client_for_collection(&self, collection: Collection) -> &RemoteSettingsClient {
        match collection {
            Collection::Allowlist => &self.allowlist_client,
            Collection::Denylist => &self.denylist_client,
        }
    }

    /// Checks if a site matches an entry in the list using PSL-aware matching.
    ///
    /// Matches on exact domain, exact host, or subdomain relationships.
    fn is_site_in_list(&self, list: &[String], host: &str, domain: &str) -> bool {
        for entry in list {
            // Exact domain or host match
            if entry == domain || entry == host {
                return true;
            }

            // Entry is a subdomain of our domain
            if entry.ends_with(domain) && entry.len() > domain.len() {
                let prefix_end = entry.len() - domain.len();
                if entry.as_bytes().get(prefix_end - 1) == Some(&b'.') {
                    return true;
                }
            }

            // Our host is a subdomain of the entry
            if host.ends_with(entry) && host.len() > entry.len() {
                let prefix_end = host.len() - entry.len();
                if host.as_bytes().get(prefix_end - 1) == Some(&b'.') {
                    return true;
                }
            }
        }

        false
    }

    /// Fetches records from a specific Remote Settings collection
    fn get_records(&self, collection: Collection) -> Result<Vec<String>> {
        let client = self.client_for_collection(collection);

        // Get records from Remote Settings
        // Use sync_if_empty=true so the first call will fetch from server if cache is empty,
        // and subsequent calls will use the cached data (fast, no network)
        let records = match client.get_records(true) {
            Some(records) => records,
            None => {
                log::debug!("No records found for collection: {}", collection.name());
                return Ok(vec![]);
            }
        };

        // If records list is empty, return empty vec (this is OK - not an error)
        if records.is_empty() {
            log::debug!("Empty records list for collection: {}", collection.name());
            return Ok(vec![]);
        }

        // Parse the records from Remote Settings
        // Remote Settings records have a `fields` property with a "domain" field
        let domains: Vec<String> = records
            .iter()
            .filter_map(|record| match record.fields.get("domain") {
                Some(value) => value.as_str().map(String::from),
                None => {
                    log::warn!(
                        "Record missing 'domain' field in {}: {:?}",
                        collection.name(),
                        record.id
                    );
                    None
                }
            })
            .collect();

        Ok(domains)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_settings::{RemoteSettingsConfig2, RemoteSettingsServer};

    // Helper to create a RemoteSettingsService for testing
    fn create_test_remote_settings_service() -> Arc<RemoteSettingsService> {
        let config = RemoteSettingsConfig2 {
            server: Some(RemoteSettingsServer::Custom {
                url: "http://localhost".to_string(),
            }),
            bucket_name: None,
            app_context: None,
        };
        Arc::new(RemoteSettingsService::new(String::from(":memory:"), config))
    }

    // Helper to create a RelayRemoteSettingsClient for testing
    fn create_test_rs_client() -> RelayRemoteSettingsClient {
        let rs_service = create_test_remote_settings_service();
        RelayRemoteSettingsClient::new(rs_service)
    }

    // Tests for is_site_in_list
    #[test]
    fn test_is_site_in_list_exact_domain_match() {
        let client = create_test_rs_client();
        let list = vec!["google.com".to_string()];
        assert!(client.is_site_in_list(&list, "google.com", "google.com"));
    }

    #[test]
    fn test_is_site_in_list_exact_host_match() {
        let client = create_test_rs_client();
        let list = vec!["mail.google.com".to_string()];
        assert!(client.is_site_in_list(&list, "mail.google.com", "google.com"));
    }

    #[test]
    fn test_is_site_in_list_subdomain_match() {
        let client = create_test_rs_client();
        let list = vec!["google.com".to_string()];
        // Subdomain should match when list has parent domain
        assert!(client.is_site_in_list(&list, "mail.google.com", "google.com"));
        assert!(client.is_site_in_list(&list, "accounts.google.com", "google.com"));
    }

    #[test]
    fn test_is_site_in_list_country_tld() {
        let client = create_test_rs_client();
        let list = vec!["google.com.ar".to_string()];
        // Should match exact domain
        assert!(client.is_site_in_list(&list, "google.com.ar", "google.com.ar"));
        // Should match subdomain
        assert!(client.is_site_in_list(&list, "accounts.google.com.ar", "google.com.ar"));
    }

    #[test]
    fn test_is_site_in_list_no_cross_domain_match() {
        let client = create_test_rs_client();
        let list = vec!["google.com.ar".to_string()];
        // Should NOT match different country TLD
        assert!(!client.is_site_in_list(&list, "google.com", "google.com"));
    }

    #[test]
    fn test_is_site_in_list_github_io() {
        let client = create_test_rs_client();
        let list = vec!["mysite.github.io".to_string()];
        // PSL treats github.io specially - each subdomain is a registrable domain
        assert!(client.is_site_in_list(&list, "mysite.github.io", "mysite.github.io"));
        // Should NOT match other github.io subdomains
        assert!(!client.is_site_in_list(&list, "other.github.io", "other.github.io"));
    }

    #[test]
    fn test_is_site_in_list_localhost() {
        let client = create_test_rs_client();
        let list = vec!["localhost".to_string()];
        assert!(client.is_site_in_list(&list, "localhost", "localhost"));
    }

    #[test]
    fn test_is_site_in_list_empty_list() {
        let client = create_test_rs_client();
        let list: Vec<String> = vec![];
        assert!(!client.is_site_in_list(&list, "google.com", "google.com"));
    }

    // Tests for should_show_relay
    // Note: These tests use an in-memory Remote Settings service with no data,
    // so allowlist and denylist will be empty unless explicitly populated.

    #[test]
    fn test_should_show_relay_empty_inputs() {
        let rs_client = create_test_rs_client();

        // Should NOT show for empty host or domain (regardless of user status)
        assert!(!rs_client.should_show_relay("".to_string(), "example.com".to_string(), false));
        assert!(!rs_client.should_show_relay("example.com".to_string(), "".to_string(), false));
        assert!(!rs_client.should_show_relay("".to_string(), "".to_string(), true));
    }

    #[test]
    fn test_should_show_relay_empty_lists_non_relay_user() {
        let rs_client = create_test_rs_client();

        // With empty allowlist and denylist, non-Relay user should NOT see promotion
        // (empty allowlist means nothing is allowed - fail-safe default)
        assert!(!rs_client.should_show_relay(
            "example.com".to_string(),
            "example.com".to_string(),
            false
        ));
    }

    #[test]
    fn test_should_show_relay_empty_lists_relay_user() {
        let rs_client = create_test_rs_client();

        // With empty denylist, existing Relay user SHOULD see Relay
        // (Relay users can use Relay on any non-denied site)
        assert!(rs_client.should_show_relay(
            "example.com".to_string(),
            "example.com".to_string(),
            true
        ));
    }

    // TODO: Add tests with actual allowlist/denylist data when we can mock Remote Settings records
    // These would test:
    // 1. Site on denylist -> never show (for both Relay and non-Relay users)
    // 2. Site not on denylist, Relay user -> always show
    // 3. Site not on denylist, non-Relay user, site on allowlist -> show promotion
    // 4. Site not on denylist, non-Relay user, site not on allowlist -> don't show
}
