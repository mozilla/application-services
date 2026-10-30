/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt;

use crate::{
    config::Application::{self, *},
    schema::FieldConfigOverrideProperty,
    util::dashboard_count_color,
};

/// Enumeration containing all Rust components.
/// When adding new variants, make sure to also update the impl block below
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    Autofill,
    Fxa,
    Logins,
    Places,
    RemoteSettings,
    Suggest,
    Tabs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SyncEngine {
    Addresses,
    Bookmarks,
    CreditCards,
    History,
    Logins,
    RustLogins,
    Tabs,
}

impl Component {
    /// Unique name for the component in slug format (lower-case letters + dashes).
    pub fn slug(&self) -> &'static str {
        match self {
            Self::Autofill => "autofill",
            Self::Fxa => "fxa",
            Self::Logins => "logins",
            Self::Places => "places",
            Self::RemoteSettings => "remote-settings",
            Self::Suggest => "suggest",
            Self::Tabs => "tabs",
        }
    }

    /// Applications your component ships on
    pub fn applications(&self) -> &[Application] {
        match self {
            Self::Autofill => &[Android, Ios],
            Self::Fxa => &[Android, Ios],
            Self::Logins => &[Desktop, Android, Ios],
            Self::Places => &[Android, Ios],
            Self::RemoteSettings => &[Desktop, Android, Ios],
            Self::Suggest => &[Desktop, Android, Ios],
            Self::Tabs => &[Desktop, Android, Ios],
        }
    }

    /// Prefix for error strings.
    ///
    /// This is the common prefix for strings sent to the `error_support`.  You can usually find it
    /// by going to `error.rs` for your component and looking at the `report_error` calls.
    pub fn error_prefix(&self) -> &'static str {
        match self {
            Self::Autofill => "autofill-",
            Self::Fxa => "fxa-client-",
            Self::Logins => "logins-",
            Self::Places => "places-",
            Self::RemoteSettings => "remote-settings-",
            Self::Suggest => "suggest-",
            Self::Tabs => "tabs-",
        }
    }

    /// Sync engine names
    ///
    /// These represent 2 things:
    ///   - The Glean pings for the component without the `-sync` suffix.
    ///   - The `engine.name` value for the legacy `telemetry.sync` table.
    pub fn sync_engines(&self) -> &[SyncEngine] {
        match self {
            Self::Autofill => &[SyncEngine::Addresses, SyncEngine::CreditCards],
            Self::Fxa => &[],
            Self::Logins => &[SyncEngine::Logins, SyncEngine::RustLogins],
            Self::Places => &[SyncEngine::Bookmarks, SyncEngine::History],
            Self::RemoteSettings => &[],
            Self::Suggest => &[],
            Self::Tabs => &[SyncEngine::Tabs],
        }
    }
}

impl SyncEngine {
    pub fn dashboard_color(&self) -> FieldConfigOverrideProperty {
        // Dashboard color for this engine.
        //
        // This method helps keep colors consistent across all panels (SYNC-5459)
        match self {
            Self::Addresses => dashboard_count_color(0, false),
            Self::Bookmarks => dashboard_count_color(1, false),
            Self::CreditCards => dashboard_count_color(2, false),
            Self::History => dashboard_count_color(3, false),
            Self::Logins => dashboard_count_color(4, false),
            Self::RustLogins => dashboard_count_color(5, false),
            Self::Tabs => dashboard_count_color(6, false),
        }
    }
}

impl fmt::Display for SyncEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Addresses => write!(f, "addresses"),
            Self::Bookmarks => write!(f, "bookmarks"),
            Self::CreditCards => write!(f, "creditcards"),
            Self::History => write!(f, "history"),
            Self::Logins => write!(f, "logins"),
            Self::RustLogins => write!(f, "rust-logins"),
            Self::Tabs => write!(f, "tabs"),
        }
    }
}
