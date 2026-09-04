/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::data::Identity;

/// How a container gets its label.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ContainerLabel {
    /// Empty only for a stored container that carries no usable label at all.
    Name { name: String },
    /// An identifier the embedder resolves against its own catalogue.
    L10nId { l10n_id: String },
}

/// A container as the embedder sees it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Container {
    pub user_context_id: u32,
    pub is_public: bool,
    pub icon: String,
    pub color: String,
    pub label: ContainerLabel,
}

impl Container {
    pub(crate) fn from_identity(identity: &Identity) -> Self {
        Self {
            user_context_id: identity.user_context_id,
            is_public: identity.public,
            icon: identity.icon.clone(),
            color: identity.color.clone(),
            label: match (&identity.name, &identity.l10n_id) {
                (Some(name), _) if !name.is_empty() => ContainerLabel::Name { name: name.clone() },
                (_, Some(l10n_id)) => ContainerLabel::L10nId {
                    l10n_id: l10n_id.clone(),
                },
                _ => ContainerLabel::Name {
                    name: String::new(),
                },
            },
        }
    }
}
