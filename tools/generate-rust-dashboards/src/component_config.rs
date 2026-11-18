/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt;

use crate::{
    config::Application, schema::FieldConfigOverrideProperty, util::dashboard_count_color,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SyncEngine {
    Addresses,
    Bookmarks,
    CreditCards,
    History,
    Logins,
    Prefs,
    RustLogins,
    Tabs,
    WebextStorage,
}

impl SyncEngine {
    /// Dashboard color for this engine.
    ///
    /// This method helps keep colors consistent across all panels (SYNC-5459)
    pub fn dashboard_color(&self) -> FieldConfigOverrideProperty {
        match self {
            Self::Addresses => dashboard_count_color(0, false),
            Self::Bookmarks => dashboard_count_color(1, false),
            Self::CreditCards => dashboard_count_color(2, false),
            Self::History => dashboard_count_color(3, false),
            Self::Logins => dashboard_count_color(4, false),
            Self::RustLogins => dashboard_count_color(5, false),
            Self::Tabs => dashboard_count_color(6, false),
            Self::Prefs => dashboard_count_color(7, false),
            Self::WebextStorage => dashboard_count_color(8, false),
        }
    }

    pub fn supported_applications(&self) -> &[Application] {
        match self {
            Self::Addresses => &[Application::Desktop, Application::Android, Application::Ios],
            Self::Bookmarks => &[Application::Desktop, Application::Android, Application::Ios],
            Self::CreditCards => &[Application::Desktop, Application::Android, Application::Ios],
            Self::History => &[Application::Desktop, Application::Android, Application::Ios],
            Self::Logins => &[Application::Android, Application::Ios],
            Self::RustLogins => &[Application::Desktop],
            Self::Tabs => &[Application::Desktop, Application::Android, Application::Ios],
            Self::Prefs => &[Application::Desktop],
            Self::WebextStorage => &[Application::Desktop],
        }
    }

    pub fn enabled_on_android(&self) -> bool {
        self.supported_applications()
            .contains(&Application::Android)
    }

    pub fn enabled_on_ios(&self) -> bool {
        self.supported_applications().contains(&Application::Ios)
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
            Self::Prefs => write!(f, "prefs"),
            Self::RustLogins => write!(f, "rust-logins"),
            Self::Tabs => write!(f, "tabs"),
            Self::WebextStorage => write!(f, "webext-storage"),
        }
    }
}
