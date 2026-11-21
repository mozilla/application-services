/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

/// How the user's choice of engines to sync is modelled.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum EngineChoicesModel {
    /// The legacy model: engine choices are shared by every device on the
    /// account, via meta/global's declined engines.
    Account {
        /// Which engine states should be *changed* in the "account global" list (for
        /// example, if the UI was used to change an engine's state since the last
        /// sync).
        /// Normally empty - only used for selection changes, does not reflect current actual choices.
        enabled_changes: HashMap<String, bool>,
    },
    /// Engine choices are specific to each device. Not yet ready for use
    /// outside of testing.
    Device,
}

/// The legacy model, with no changes.
impl Default for EngineChoicesModel {
    fn default() -> Self {
        Self::Account {
            enabled_changes: HashMap::new(),
        }
    }
}

impl EngineChoicesModel {
    /// The changes to make to the account-wide engine choices, if there are any.
    pub fn enabled_changes(&self) -> Option<&HashMap<String, bool>> {
        match self {
            Self::Account { enabled_changes } if !enabled_changes.is_empty() => {
                Some(enabled_changes)
            }
            _ => None,
        }
    }
}

/// The account-wide engine choices, as recorded in `meta/global`. Maintained by
/// clients using `EngineChoicesModel::Account`, but reported with either model,
/// so apps using per-device choices can adopt them.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct AccountEngineChoices {
    /// The engines in `meta/global`'s engines.
    pub enabled: Vec<String>,
    /// The engines in `meta/global`'s declined.
    pub declined: Vec<String>,
}
