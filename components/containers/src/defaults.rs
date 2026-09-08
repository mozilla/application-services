/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_json::{Map, Value};

use crate::container::ContainerLabel;
use crate::data::{ContainersData, Identity, LATEST_VERSION, MAX_USER_CONTEXT_ID};
use crate::definitions;
use crate::error::InitError;

pub(crate) const THUMBNAIL_IDENTITY_NAME: &str = "userContextIdInternal.thumbnail";
pub(crate) const WEBEXT_STORAGE_LOCAL_IDENTITY_NAME: &str =
    "userContextIdInternal.webextStorageLocal";

/// A public identity to seed a fresh store with. Enterprise policy can replace
/// the shipped set, so the caller may supply its own.
///
/// Icon and color are validated when the store is opened rather than when the
/// value is built, so that this stays a plain record the embedder constructs
/// directly.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UserIdentitySpec {
    pub icon: String,
    pub color: String,
    pub label: ContainerLabel,
}

impl UserIdentitySpec {
    /// Rejects an icon or color the crate cannot render, resolving legacy color
    /// names the way the WebExtension boundary does.
    pub(crate) fn validated(&self) -> Result<Self, InitError> {
        if !definitions::is_known_icon(&self.icon) {
            return Err(InitError::InvalidSeedIcon {
                icon: self.icon.clone(),
            });
        }

        let color = definitions::canonical_color(&self.color).ok_or_else(|| {
            InitError::InvalidSeedColor {
                color: self.color.clone(),
            }
        })?;

        Ok(Self {
            color,
            ..self.clone()
        })
    }

    fn localized(icon: &str, color: &str, l10n_id: &str) -> Self {
        Self {
            icon: icon.to_string(),
            color: color.to_string(),
            label: ContainerLabel::L10nId {
                l10n_id: l10n_id.to_string(),
            },
        }
    }
}

pub(crate) fn shipped_user_identities() -> Vec<UserIdentitySpec> {
    vec![
        UserIdentitySpec::localized("fingerprint", "blue", "user-context-personal"),
        UserIdentitySpec::localized("briefcase", "orange", "user-context-work"),
        UserIdentitySpec::localized("dollar", "green", "user-context-banking"),
        UserIdentitySpec::localized("cart", "pink", "user-context-shopping"),
    ]
}

/// The system identities still carry an empty `accessKey`, which predates the
/// move to Fluent. Kept so that a freshly seeded store matches what Firefox
/// writes today.
fn system_identity(user_context_id: u32, name: &str) -> Identity {
    let mut extra = Map::new();
    extra.insert("accessKey".to_string(), Value::String(String::new()));

    Identity {
        user_context_id,
        public: false,
        icon: String::new(),
        color: String::new(),
        name: Some(name.to_string()),
        l10n_id: None,
        extra,
    }
}

pub(crate) fn thumbnail_identity(user_context_id: u32) -> Identity {
    system_identity(user_context_id, THUMBNAIL_IDENTITY_NAME)
}

pub(crate) fn webext_storage_local_identity() -> Identity {
    system_identity(MAX_USER_CONTEXT_ID, WEBEXT_STORAGE_LOCAL_IDENTITY_NAME)
}

pub(crate) fn defaults() -> ContainersData {
    defaults_with(&shipped_user_identities())
}

pub(crate) fn defaults_with(user_identities: &[UserIdentitySpec]) -> ContainersData {
    let mut identities = Vec::with_capacity(user_identities.len() + 2);
    let mut next_user_context_id = 1;

    for spec in user_identities {
        let (name, l10n_id) = match &spec.label {
            ContainerLabel::Name { name } => (Some(name.clone()), None),
            ContainerLabel::L10nId { l10n_id } => (None, Some(l10n_id.clone())),
        };

        identities.push(Identity {
            user_context_id: next_user_context_id,
            public: true,
            icon: spec.icon.clone(),
            color: spec.color.clone(),
            name,
            l10n_id,
            extra: Map::new(),
        });
        next_user_context_id += 1;
    }

    identities.push(thumbnail_identity(next_user_context_id));
    let last_user_context_id = next_user_context_id;
    identities.push(webext_storage_local_identity());

    ContainersData {
        version: LATEST_VERSION,
        last_user_context_id,
        identities,
        site_associations: Default::default(),
        extra: Map::new(),
    }
}
