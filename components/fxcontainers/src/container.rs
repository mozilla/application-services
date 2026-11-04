/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::data::Identity;
use crate::definitions::{self, ContainerColor, ContainerIcon, ContainerLabel};

/// A container as the embedder sees it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Container {
    pub user_context_id: u32,
    pub is_public: bool,
    pub icon: Option<ContainerIcon>,
    pub color: Option<ContainerColor>,
    pub label: ContainerLabel,
    pub policy_id: Option<String>,
}

impl Container {
    /// `default_label` is the label of the default identity that owns this
    /// container's id, if any. It labels a default container the user has not
    /// renamed, since the label of a default is not stored.
    pub(crate) fn from_identity(
        identity: &Identity,
        default_label: Option<&ContainerLabel>,
    ) -> Self {
        Self {
            user_context_id: identity.user_context_id,
            is_public: identity.public,
            icon: definitions::icon_from_name(&identity.icon),
            color: definitions::color_from_name(&identity.color),
            policy_id: identity
                .policy
                .then(|| identity.policy_id.clone())
                .flatten(),
            label: match &identity.name {
                Some(name) if !name.is_empty() => ContainerLabel::Name { name: name.clone() },
                _ => default_label.cloned().unwrap_or(ContainerLabel::Name {
                    name: String::new(),
                }),
            },
        }
    }
}
