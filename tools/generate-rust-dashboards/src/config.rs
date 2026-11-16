/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt;

pub use crate::schema::Unit;
use crate::util::slug;

/// Dashboard configuration for a team
pub struct TeamConfig {
    /// Display name for the team.
    ///
    /// This is what shows up on your dashboard titles.  Spell it out however you want.
    pub team_name: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Application {
    Android,
    Ios,
    Desktop,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReleaseChannel {
    Nightly,
    Beta,
    Release,
}

impl TeamConfig {
    pub fn team_slug(&self) -> String {
        slug(self.team_name)
    }

    pub fn dashboard_slug(&self, dashboard_slug: &str) -> String {
        format!("{}-{dashboard_slug}", self.team_slug())
    }
}

impl Application {
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Desktop, Self::Android, Self::Ios].into_iter()
    }

    pub fn slug(&self) -> &'static str {
        match self {
            Self::Android => "firefox_android",
            Self::Ios => "firefox_ios",
            Self::Desktop => "firefox_desktop",
        }
    }

    pub fn bigquery_dataset(&self) -> &'static str {
        // There's a few datasets we can use, these were chosen because they seem to include
        // release, beta, and nightly data
        match self {
            Self::Android => "fenix",
            Self::Ios => "firefox_ios",
            Self::Desktop => "firefox_desktop",
        }
    }

    pub fn display_name(&self, channel: ReleaseChannel) -> String {
        format!("{self} ({channel})")
    }
}

impl ReleaseChannel {
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Nightly, Self::Beta, Self::Release].into_iter()
    }

    pub fn slug(&self) -> &'static str {
        match self {
            Self::Nightly => "nightly",
            Self::Beta => "beta",
            Self::Release => "release",
        }
    }
}

impl fmt::Display for Application {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Android => write!(f, "Android"),
            Self::Ios => write!(f, "iOS"),
            Self::Desktop => write!(f, "Desktop"),
        }
    }
}

impl fmt::Display for ReleaseChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nightly => write!(f, "nightly"),
            Self::Beta => write!(f, "beta"),
            Self::Release => write!(f, "release"),
        }
    }
}
