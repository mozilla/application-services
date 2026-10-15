/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Core event primitives: the event, the per-field gate key, the total order
//! deciding which of two concurrent events wins, and the on-wire event
//! representation (`Wire`, `WireEvent`, `WirePart`).
//!
//! Events are opaque and GUID-keyed. Each has a record id (the server key), a
//! [`GateKey`] (the caller-defined type plus the entity the event is about),
//! and a schema-versioned [`crate::schema::Body`]. The parts of a split record
//! add `_{index}` to the base id (see [`batch_record_id`]).
//!
//! [`IncomingEvent`] is the assembled shape the engine consumes. `WireEvent`
//! and `WirePart` are how a record is serialized and (when too large) framed
//! for transport. The base64 framing and reassembly of frames lives in the engine.

use crate::schema::{Body, SchemaVersion};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
use sync15::{Guid, ServerTimestamp};

/// Reserved event type under which a baseline is schema-migrated.
pub const BASELINE_EVENT_TYPE: &str = "__baseline__";

/// Reserved event type for a `baseline-needed` marker (asks a schema-capable
/// client to write a baseline).
pub const BASELINE_NEEDED_EVENT_TYPE: &str = "__baseline_needed__";

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
/// Newer means larger `modified`, ties broken by record id. Records in one
/// upload batch share a `modified`; their ids are immutable and the same for
/// everyone, so the tie-break gives every client the same order.
///
/// Serializes as `{ "modified_ms", "record_id" }` with explicit integer millis
/// (not `ServerTimestamp`'s float seconds), so a persisted or on-wire position
/// is always parseable independent of the caller's payload schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventPosition {
    #[serde(rename = "modified_ms", with = "millis")]
    pub modified: ServerTimestamp,
    pub record_id: Guid,
}

/// serde adapter: `ServerTimestamp` as integer milliseconds.
mod millis {
    use super::*;

    pub fn serialize<S: Serializer>(
        ts: &ServerTimestamp,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_i64(ts.as_millis())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<ServerTimestamp, D::Error> {
        i64::deserialize(deserializer).map(ServerTimestamp::from_millis)
    }
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

/// An incoming event read from the server, reassembled from its transport frames
/// if it was split, before its body is resolved to a single schema.
///
/// The single shape the gate and the baseline path both consume. A baseline is
/// just an event with a reserved type. A baseline also carries `covers_up_to`:
/// the point in the global order it stands in for, which advances the floor (its
/// own `position` is just where the record landed).
#[derive(Clone, Debug, PartialEq)]
pub struct IncomingEvent {
    /// The event type and entity: what the event is, and what it is about.
    pub gate: GateKey,
    /// The record's place in the server order: `(modified, group-or-own-id)`.
    pub position: EventPosition,
    /// The schema-versioned payload, whole even if the record was split for
    /// transport (splitting and reassembly happen on raw bytes, below this).
    pub body: Body,
    /// Highest schema version tag the body carries (visible even when this client
    /// can't read that schema).
    pub max_schema: SchemaVersion,
    /// The covered position, for a baseline; `None` for every other event.
    pub covers_up_to: Option<EventPosition>,
}

/// A whole logical record, distinguished by `event_type`: a domain event, a
/// baseline ([`BASELINE_EVENT_TYPE`]), or a `baseline-needed` marker
/// ([`BASELINE_NEEDED_EVENT_TYPE`]). Record id and `modified` live in the BSO
/// envelope; the rest is the payload.
///
/// A record too large for one upload is serialized and cut into [`WirePart`]
/// frames instead. That is pure transport, below this shape.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WireEvent {
    #[serde(rename = "type")]
    pub(crate) event_type: String,
    /// The entity the event is about. Empty (and omitted) for baselines/markers.
    #[serde(default = "empty_guid", skip_serializing_if = "guid_is_empty")]
    pub(crate) entity: Guid,
    /// The point in the global order a baseline stands in for. Baselines only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) covers_up_to: Option<EventPosition>,
    /// Emitter schema on a `baseline-needed` marker, so the fleet can elect its
    /// highest-schema member. Markers only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) schema: Option<SchemaVersion>,
    /// The schema-versioned payload. Absent on a `baseline-needed` marker.
    #[serde(default, skip_serializing_if = "Body::is_empty")]
    pub(crate) body: Body,
}

/// One `_{n}` frame of a record too big for a single upload: an opaque,
/// base64-encoded slice of the serialized [`WireEvent`]. Frames sharing a `group`
/// reassemble in `part` order, base64-decoding `data`, back into the original
/// record's bytes. The engine never inspects the payload; encoding lets it cut the
/// bytes at any offset without minding UTF-8 boundaries.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WirePart {
    pub(crate) group: Guid,
    pub(crate) part: u32,
    pub(crate) part_count: u32,
    pub(crate) data: String,
}

/// A record read from the server: either whole, or one frame of a split one.
/// Untagged, so a whole record stays lean (no wrapper) on the wire.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Wire {
    Whole(WireEvent),
    Frame(WirePart),
}

fn empty_guid() -> Guid {
    Guid::new("")
}
fn guid_is_empty(g: &Guid) -> bool {
    g.as_str().is_empty()
}

impl WireEvent {
    /// A domain event.
    pub(crate) fn event(event_type: String, entity: Guid, body: Body) -> Self {
        Self {
            event_type,
            entity,
            covers_up_to: None,
            schema: None,
            body,
        }
    }

    /// A baseline covering `covers_up_to`.
    pub(crate) fn baseline(covers_up_to: EventPosition, body: Body) -> Self {
        Self {
            event_type: BASELINE_EVENT_TYPE.to_string(),
            entity: empty_guid(),
            covers_up_to: Some(covers_up_to),
            schema: None,
            body,
        }
    }

    /// A `baseline-needed` marker advertising the emitter's schema.
    pub(crate) fn baseline_needed(schema: SchemaVersion) -> Self {
        Self {
            event_type: BASELINE_NEEDED_EVENT_TYPE.to_string(),
            entity: empty_guid(),
            covers_up_to: None,
            schema: Some(schema),
            body: Body::new(),
        }
    }
}

/// Record id for the `index`th frame of a split record sharing a base id (`_{X}`).
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
    fn position_serializes_as_integer_millis_and_round_trips() {
        let pos = EventPosition::new(ServerTimestamp::from_millis(1234), Guid::new("rec-1"));
        let json = serde_json::to_string(&pos).unwrap();
        // Integer millis, not `ServerTimestamp`'s float seconds.
        assert_eq!(json, r#"{"modified_ms":1234,"record_id":"rec-1"}"#);
        let parsed: EventPosition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, pos);
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
