/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_json::Map;

use crate::data::{ContainersData, Identity, LATEST_VERSION, MAX_USER_CONTEXT_ID};
use crate::definitions::{ContainerColor, ContainerIcon, ContainerLabel};

pub(crate) const THUMBNAIL_IDENTITY_NAME: &str = "userContextIdInternal.thumbnail";
pub(crate) const WEBEXT_STORAGE_LOCAL_IDENTITY_NAME: &str =
    "userContextIdInternal.webextStorageLocal";

/// One of the default containers the store was opened with, shipped or supplied
/// by the embedder. Enterprise policy can replace the shipped set, and names its
/// containers rather than localizing them, so its entries carry a
/// [`ContainerLabel::Name`].
///
/// They seed a fresh store, and they label the default containers the user has
/// not renamed: that label is never stored, so that renaming the string behind
/// it does not need a migration.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DefaultIdentity {
    pub icon: ContainerIcon,
    pub color: ContainerColor,
    pub label: ContainerLabel,
}

impl DefaultIdentity {
    fn new(icon: ContainerIcon, color: ContainerColor, label: ContainerLabel) -> Self {
        Self { icon, color, label }
    }
}

pub(crate) fn shipped_defaults() -> Vec<DefaultIdentity> {
    vec![
        DefaultIdentity::new(
            ContainerIcon::Fingerprint,
            ContainerColor::Blue,
            ContainerLabel::Personal,
        ),
        DefaultIdentity::new(
            ContainerIcon::Briefcase,
            ContainerColor::Orange,
            ContainerLabel::Work,
        ),
        DefaultIdentity::new(
            ContainerIcon::Dollar,
            ContainerColor::Green,
            ContainerLabel::Banking,
        ),
        DefaultIdentity::new(
            ContainerIcon::Cart,
            ContainerColor::Pink,
            ContainerLabel::Shopping,
        ),
    ]
}

/// The label of the default identity that owns `user_context_id`, or `None`
/// when no default does. The nth default owns the nth id, as [`defaults_with`]
/// hands them out.
pub(crate) fn default_label(
    defaults: &[DefaultIdentity],
    user_context_id: u32,
) -> Option<&ContainerLabel> {
    let index = usize::try_from(user_context_id.checked_sub(1)?).ok()?;
    defaults.get(index).map(|default| &default.label)
}

fn system_identity(user_context_id: u32, name: &str) -> Identity {
    Identity {
        user_context_id,
        public: false,
        icon: String::new(),
        color: String::new(),
        name: Some(name.to_string()),
        policy: false,
        policy_id: None,
        extra: Map::new(),
    }
}

pub(crate) fn thumbnail_identity(user_context_id: u32) -> Identity {
    system_identity(user_context_id, THUMBNAIL_IDENTITY_NAME)
}

pub(crate) fn webext_storage_local_identity() -> Identity {
    system_identity(MAX_USER_CONTEXT_ID, WEBEXT_STORAGE_LOCAL_IDENTITY_NAME)
}

#[cfg(test)]
pub(crate) fn defaults() -> ContainersData {
    defaults_with(&shipped_defaults())
}

pub(crate) fn defaults_with(defaults: &[DefaultIdentity]) -> ContainersData {
    let mut identities = Vec::with_capacity(defaults.len() + 2);
    let mut next_user_context_id = 1;

    for default in defaults {
        let name = match &default.label {
            ContainerLabel::Name { name } => Some(name.clone()),
            _ => None,
        };

        identities.push(Identity {
            user_context_id: next_user_context_id,
            public: true,
            icon: default.icon.name().to_string(),
            color: default.color.name().to_string(),
            name,
            policy: false,
            policy_id: None,
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
