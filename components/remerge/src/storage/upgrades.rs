/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::RecordSchema;
use std::collections::HashSet;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UpgradeKind {
    Trivial,
    RequiresDedupe,
    // TODO: Ignoring incompatible migrations for now...
}

impl UpgradeKind {
    pub fn between(src: &RecordSchema, dst: &RecordSchema) -> UpgradeKind {
        let our_keys = src.raw.dedupe_on.iter().collect::<HashSet<_>>();
        let their_keys = dst.raw.dedupe_on.iter().collect::<HashSet<_>>();
        let has_new = their_keys.difference(&our_keys).any(|_| true);

        if has_new {
            UpgradeKind::RequiresDedupe
        } else {
            UpgradeKind::Trivial
        }
    }
}
