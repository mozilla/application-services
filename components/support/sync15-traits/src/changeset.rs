/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{Guid, Payload, ServerTimestamp};

#[derive(Debug, Clone, Default)]
pub struct RecordChangeset<P> {
    pub changes: Vec<P>,
    /// For GETs, the last sync timestamp that should be persisted after
    /// applying the records.
    /// For POSTs, this is the XIUS timestamp.
    pub timestamp: ServerTimestamp,
    pub collection: std::borrow::Cow<'static, str>,
}

pub type IncomingChangeset = RecordChangeset<(Payload, ServerTimestamp)>;
pub type OutgoingChangeset = RecordChangeset<Payload>;

// TODO: use a trait to unify this with the non-json versions
impl<T> RecordChangeset<T> {
    #[inline]
    pub fn new(
        collection: impl Into<std::borrow::Cow<'static, str>>,
        timestamp: ServerTimestamp,
    ) -> RecordChangeset<T> {
        RecordChangeset {
            changes: vec![],
            timestamp,
            collection: collection.into(),
        }
    }
}
impl IncomingChangeset {
    pub fn find<T: ?Sized>(&self, id: &T) -> Option<&Payload>
    where
        Guid: PartialEq<T>,
    {
        self.changes.iter().map(|p| &p.0).find(|p| p.id == *id)
    }
}
