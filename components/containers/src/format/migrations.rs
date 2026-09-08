/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_json::Value;

use crate::data::ContainersData;
use crate::defaults;
use crate::definitions;

/// Bug 1419591: nothing to rewrite, the version alone had to move.
pub(crate) fn migrate_2_to_3(data: &mut ContainersData) {
    data.version = 3;
}

/// Bug 1406181: reserve the identity backing the extension storage.local API.
pub(crate) fn migrate_3_to_4(data: &mut ContainersData) {
    data.identities
        .push(defaults::webext_storage_local_identity());
    data.version = 4;
}

/// Bug 1814969: StringBundle labels give way to Fluent identifiers.
pub(crate) fn migrate_4_to_5(data: &mut ContainersData) {
    for identity in &mut data.identities {
        let legacy = identity.extra.remove("l10nID");
        identity.extra.remove("accessKey");

        let Some(Value::String(legacy)) = legacy else {
            continue;
        };

        // Anything outside the four shipped labels keeps whatever it had.
        let fluent = match legacy.as_str() {
            "userContextPersonal.label" => Some("user-context-personal"),
            "userContextWork.label" => Some("user-context-work"),
            "userContextBanking.label" => Some("user-context-banking"),
            "userContextShopping.label" => Some("user-context-shopping"),
            _ => None,
        };

        if let Some(fluent) = fluent {
            identity.l10n_id = Some(fluent.to_string());
        }
    }

    data.version = 5;
}

/// The color refresh: stored identities keep only canonical names.
pub(crate) fn migrate_5_to_6(data: &mut ContainersData) {
    for identity in &mut data.identities {
        if !identity.color.is_empty() {
            identity.color = definitions::resolve_color(&identity.color);
        }
    }

    data.version = 6;
}
