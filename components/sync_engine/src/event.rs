/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Core event primitives: the event, the per-field gate key, and the total
//! order deciding which of two concurrent events wins.
//!
//! Events are opaque and GUID-keyed. Each has a record id (server key; batches
//! suffix the base UUID with `_{X}`, see [`batch_record_id`]), an entity (what
//! the event is about; with the type it forms the [`GateKey`]), a caller-defined
//! type, and a schema-versioned [`crate::schema::EventBody`].

use crate::schema::EventBody;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use sync15::{Guid, ServerTimestamp};

/// The identity a last-seen entry is scoped to. Conflict resolution is per
/// `(event type, entity)`: a `modify-name` on X is ordered independently from a
/// `modify-color` on X, or a `modify-name` on Y.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GateKey {
    pub event_type: String,
    pub entity: Guid,
}

impl GateKey {
    pub fn new(event_type: impl Into<String>, entity: Guid) -> Self {
        Self {
            event_type: event_type.into(),
            entity,
        }
    }
}

/// An event's position in the global server order.
///
/// Newer means larger `modified`, ties broken by record id. Batch operations
/// share a `modified` and are disambiguated by their `_{X}`-suffixed ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventPosition {
    pub modified: ServerTimestamp,
    pub record_id: Guid,
}

impl EventPosition {
    pub fn new(modified: ServerTimestamp, record_id: Guid) -> Self {
        Self {
            modified,
            record_id,
        }
    }

    /// True if `self` is strictly newer than `other`. Equal positions do not
    /// supersede, so re-applying the same event is a no-op (idempotent).
    pub fn supersedes(&self, other: &EventPosition) -> bool {
        self > other
    }
}

impl Ord for EventPosition {
    fn cmp(&self, other: &Self) -> Ordering {
        // By millis, then record id (`ServerTimestamp` isn't `Ord`).
        self.modified
            .as_millis()
            .cmp(&other.modified.as_millis())
            .then_with(|| self.record_id.as_str().cmp(other.record_id.as_str()))
    }
}

impl PartialOrd for EventPosition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// An event authored locally or read from the server, before its body is
/// resolved to a single schema representation.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub event_type: String,
    pub entity: Guid,
    pub body: EventBody,
}

impl Event {
    pub fn new(event_type: impl Into<String>, entity: Guid, body: EventBody) -> Self {
        Self {
            event_type: event_type.into(),
            entity,
            body,
        }
    }

    pub fn gate_key(&self) -> GateKey {
        GateKey::new(self.event_type.clone(), self.entity.clone())
    }
}

/// Record id for the `index`th event of a batch sharing a base UUID (`_{X}`).
pub fn batch_record_id(base: &Guid, index: usize) -> Guid {
    Guid::new(&format!("{}_{}", base.as_str(), index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(millis: i64, id: &str) -> EventPosition {
        EventPosition::new(ServerTimestamp::from_millis(millis), Guid::new(id))
    }

    #[test]
    fn newer_timestamp_supersedes() {
        // name changed at 200 then 210.
        let older = pos(200, "X");
        let newer = pos(210, "X");
        assert!(newer.supersedes(&older));
        assert!(!older.supersedes(&newer));
    }

    #[test]
    fn equal_position_is_idempotent() {
        let a = pos(200, "X");
        let b = pos(200, "X");
        assert!(!a.supersedes(&b));
        assert!(!b.supersedes(&a));
    }

    #[test]
    fn ties_break_on_record_id() {
        // Same `modified`, disambiguated by the record id suffix.
        let base = Guid::new("abc");
        let first =
            EventPosition::new(ServerTimestamp::from_millis(300), batch_record_id(&base, 0));
        let second =
            EventPosition::new(ServerTimestamp::from_millis(300), batch_record_id(&base, 1));
        assert!(second.supersedes(&first));
        assert!(!first.supersedes(&second));
    }
}
