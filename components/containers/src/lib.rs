/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(unreachable_pub)]

//! Storage for Firefox containers.
//!
//! The crate owns the container list, the shape of the document it is stored
//! in, and the migrations between that document's versions. It does not own the
//! storage itself and never touches the filesystem: [`ContainersStore`] hands
//! the serialized bytes to its callback, and where they end up and how durably
//! is the embedder's decision.

uniffi::setup_scaffolding!("containers");

mod container;
mod data;
mod defaults;
mod definitions;
mod error;
mod format;
mod store;

pub use container::{Container, ContainerLabel};
pub use defaults::UserIdentitySpec;
pub use definitions::{
    color_code, color_l10n_id, container_color_aliases, container_colors, container_icons,
    icon_l10n_id, is_known_icon, resolve_color, ContainerColor, ContainerIcon,
};
pub use error::{InitError, StoreError};
pub use store::{normalize_site, ContainersCallback, ContainersStore, SiteAssociation};

#[uniffi::export]
pub fn latest_version() -> u32 {
    data::LATEST_VERSION
}

#[uniffi::export]
pub fn max_user_context_id() -> u32 {
    data::MAX_USER_CONTEXT_ID
}
