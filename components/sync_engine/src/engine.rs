/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! The generic event-based sync engine.
//!
//! [`EventSyncEngine`] wires together the [`EventReducer`] (storage), the
//! [`SchemaRegistry`] (schema evolution), the [`MetadataStore`] (last-seen gate
//! and baseline bookkeeping), and the [`sync15::engine::SyncEngine`] transport.
//!
//! Incoming events are ordered, gated per `(type, entity)`, resolved to the
//! current schema, and dispatched to the reducer. Local changes are recorded via
//! [`EventSyncEngine::record_local_event`] and uploaded on the next sync.
//!
//! Baselines (see [`crate::baseline`]) bound the event stream: a new client
//! cold-starts from the newest readable baseline; any client advances its floor
//! and prunes when it observes one; a client past the cadence writes a new
//! baseline, or a `baseline-needed` marker if it's below the schema bar.

use crate::baseline::{split_snapshot, BaselineConfig, WirePosition, SNAPSHOT_EVENT_TYPE};
use crate::error::{Error, Result};
use crate::event::{batch_record_id, Event, EventPosition, GateKey};
use crate::metadata::MetadataStore;
use crate::reducer::{EventReducer, ResolvedEvent, SnapshotEntity};
use crate::schema::{EventBody, SchemaRegistry, SchemaVersion};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use sync15::bso::{IncomingBso, IncomingKind, OutgoingBso, OutgoingEnvelope};
use sync15::engine::{CollectionRequest, EngineSyncAssociation, SyncEngine};
use sync15::{CollectionName, Guid, ServerTimestamp};

/// The wire representation of a record. Record id and `modified` live in the BSO
/// envelope; the rest is the payload, tagged by `record` so events, baselines
/// and markers share one collection.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "kebab-case")]
enum WireRecord {
    /// A single event; `body` is the schema-versioned payload.
    Event {
        #[serde(rename = "type")]
        event_type: String,
        entity: Guid,
        body: EventBody,
    },
    /// One part of a baseline: the full state as of `covers_up_to`, under
    /// [`SNAPSHOT_EVENT_TYPE`]. A baseline too big for one record is split into
    /// `part_count` parts that share a `group`, with ids `group_0 ..`. A reader
    /// waits until it has every part before using them. A small baseline is one
    /// part (`part_count == 1`).
    Baseline {
        group: Guid,
        part: u32,
        part_count: u32,
        covers_up_to: WirePosition,
        body: EventBody,
    },
    /// Asks a schema-capable client to write a baseline. Carries the emitter's
    /// schema so the fleet can elect its highest-schema member.
    BaselineNeeded { schema: SchemaVersion },
}

/// A local event awaiting upload, remembered so [`SyncEngine::set_uploaded`] can
/// record its final server position into the gate.
struct PendingOutgoing {
    gate: GateKey,
    bso: OutgoingBso,
}

/// A baseline we wrote. We wait for [`SyncEngine::set_uploaded`] to confirm
/// every part before advancing the floor, so a half-uploaded baseline never
/// moves the floor or prunes the gate.
struct PendingBaseline {
    cover: EventPosition,
    remaining: HashSet<Guid>,
}

/// A complete baseline reassembled from its `_N` parts, ordered by part index.
struct StagedBaseline {
    cover: EventPosition,
    parts: Vec<EventBody>,
}

pub struct EventSyncEngine {
    collection: CollectionName,
    reducer: Box<dyn EventReducer>,
    schema: SchemaRegistry,
    meta: Box<dyn MetadataStore>,
    baseline_config: BaselineConfig,

    /// Incoming records staged by `stage_incoming`, drained by `apply`.
    staged: Mutex<Vec<IncomingBso>>,
    /// Local events recorded but not yet handed to `apply`.
    outgoing: Mutex<Vec<PendingOutgoing>>,
    /// Records awaiting upload confirmation, keyed by id so `set_uploaded` gates them.
    awaiting_upload: Mutex<HashMap<Guid, GateKey>>,
    /// Baselines we wrote, keyed by group id, awaiting full-upload confirmation.
    pending_baselines: Mutex<HashMap<Guid, PendingBaseline>>,
}

impl EventSyncEngine {
    pub fn new(
        collection: impl Into<CollectionName>,
        reducer: Box<dyn EventReducer>,
        schema: SchemaRegistry,
        meta: Box<dyn MetadataStore>,
    ) -> Self {
        Self {
            collection: collection.into(),
            reducer,
            schema,
            meta,
            baseline_config: BaselineConfig::default(),
            staged: Mutex::new(Vec::new()),
            outgoing: Mutex::new(Vec::new()),
            awaiting_upload: Mutex::new(HashMap::new()),
            pending_baselines: Mutex::new(HashMap::new()),
        }
    }

    /// Override the baseline cadence configuration.
    pub fn with_baseline_config(mut self, config: BaselineConfig) -> Self {
        self.baseline_config = config;
        self
    }

    /// Record a local change as an event to upload on the next sync.
    ///
    /// `payload` is in the client's current schema; the engine dual-writes it
    /// down to the floor. The gate advances in `set_uploaded`, once the server
    /// assigns the authoritative `modified` timestamp.
    pub fn record_local_event(
        &self,
        event_type: &str,
        entity: Guid,
        payload: serde_json::Value,
    ) -> Result<()> {
        let body = self.schema.dual_write(event_type, payload)?;
        let record = WireRecord::Event {
            event_type: event_type.to_string(),
            entity: entity.clone(),
            body,
        };
        let bso = OutgoingBso::from_content(OutgoingEnvelope::from(Guid::random()), &record)?;
        let gate = GateKey::new(event_type, entity);
        self.outgoing
            .lock()
            .unwrap()
            .push(PendingOutgoing { gate, bso });
        Ok(())
    }

    /// Apply an incoming event through the gate. Returns whether it won and was
    /// dispatched to the reducer.
    ///
    /// Feed events in ascending server order so the gate advances monotonically;
    /// [`EventSyncEngine::apply`] does this.
    pub fn ingest_event(
        &self,
        record_id: Guid,
        modified: ServerTimestamp,
        event: Event,
    ) -> Result<bool> {
        let floor = self.meta.baseline_state()?.floor;
        self.ingest_with_floor(
            EventPosition::new(modified, record_id),
            event,
            floor.as_ref(),
        )
    }

    /// Gate + dispatch an event against an explicit baseline floor.
    fn ingest_with_floor(
        &self,
        incoming: EventPosition,
        event: Event,
        floor: Option<&EventPosition>,
    ) -> Result<bool> {
        // Ignore anything superseded by a baseline.
        if let Some(floor) = floor {
            if !incoming.supersedes(floor) {
                return Ok(false);
            }
        }

        let gate = event.gate_key();
        if let Some(stored) = self.meta.position(&gate)? {
            if !incoming.supersedes(&stored) {
                // Older than or identical to what we already applied.
                return Ok(false);
            }
        }

        // Migrate-on-read to our schema. An unreadable event is below our floor:
        // skip it without advancing the gate.
        let payload = match self
            .schema
            .read(&event.event_type, &event.body, self.schema.current())
        {
            Ok(payload) => payload,
            Err(Error::NoReadableSchema { .. }) => return Ok(false),
            Err(e) => return Err(e),
        };

        self.reducer.apply_event(ResolvedEvent {
            event_type: event.event_type,
            entity: event.entity,
            payload,
        });
        self.meta.set_position(gate, incoming)?;
        Ok(true)
    }

    /// Read each part at the current schema and join them, in order, into the
    /// full snapshot. `None` if any part has no schema this client can read (the
    /// baseline is then below our floor).
    fn read_snapshot_parts(&self, parts: &[EventBody]) -> Result<Option<Vec<SnapshotEntity>>> {
        let mut entities = Vec::new();
        for body in parts {
            match self
                .schema
                .read(SNAPSHOT_EVENT_TYPE, body, self.schema.current())
            {
                Ok(payload) => {
                    entities.extend(serde_json::from_value::<Vec<SnapshotEntity>>(payload)?)
                }
                Err(Error::NoReadableSchema { .. }) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
        Ok(Some(entities))
    }

    /// Serialize the reducer's current snapshot into baseline records, splitting
    /// it across `group_0 ..` parts so each stays under the record budget.
    /// Returns the group id and its parts.
    fn build_baseline_bsos(
        &self,
        covers_up_to: &EventPosition,
    ) -> Result<(Guid, Vec<OutgoingBso>)> {
        let entities = self.reducer.snapshot();
        let group = Guid::random();
        let cover_wire = WirePosition::from(covers_up_to);

        // Measure a candidate part by the byte size of its dual-written record
        // (what actually ships). On a dual-write error we return `MAX` so the
        // splitter stops growing the part; the real error surfaces when we build
        // the parts below.
        let measure = |chunk: &[SnapshotEntity]| -> usize {
            let Ok(value) = serde_json::to_value(chunk) else {
                return usize::MAX;
            };
            let Ok(body) = self.schema.dual_write(SNAPSHOT_EVENT_TYPE, value) else {
                return usize::MAX;
            };
            let record = WireRecord::Baseline {
                group: group.clone(),
                part: 0,
                part_count: 0,
                covers_up_to: cover_wire.clone(),
                body,
            };
            serde_json::to_vec(&record)
                .map(|v| v.len())
                .unwrap_or(usize::MAX)
        };

        let chunks = split_snapshot(
            entities,
            self.baseline_config.max_record_payload_bytes,
            measure,
        );
        let part_count = chunks.len() as u32;
        let mut bsos = Vec::with_capacity(chunks.len());
        for (i, chunk) in chunks.into_iter().enumerate() {
            let body = self
                .schema
                .dual_write(SNAPSHOT_EVENT_TYPE, serde_json::to_value(&chunk)?)?;
            let record = WireRecord::Baseline {
                group: group.clone(),
                part: i as u32,
                part_count,
                covers_up_to: cover_wire.clone(),
                body,
            };
            let id = batch_record_id(&group, i);
            bsos.push(OutgoingBso::from_content(
                OutgoingEnvelope::from(id),
                &record,
            )?);
        }
        Ok((group, bsos))
    }

    fn build_baseline_needed_bso(&self) -> Result<OutgoingBso> {
        Ok(OutgoingBso::from_content(
            OutgoingEnvelope::from(Guid::random()),
            &WireRecord::BaselineNeeded {
                schema: self.schema.current(),
            },
        )?)
    }
}

/// The newer of the two positions (or `b` if there's no previous one).
fn max_position(prev: Option<EventPosition>, b: EventPosition) -> EventPosition {
    match prev {
        Some(p) if p >= b => p,
        _ => b,
    }
}

/// Join a group's parts into a complete baseline, or `None` if parts are missing
/// or disagree. We skip a partial group until it is whole, so a half-visible
/// baseline never advances the floor.
fn assemble_baseline(
    mut parts: Vec<(u32, u32, EventPosition, EventBody)>,
) -> Option<StagedBaseline> {
    // Sort by the part number so `_10` comes after `_2` (sorting by record id
    // would not). Then require the parts to be exactly `0..count`, with one
    // agreed count and one cover.
    parts.sort_by_key(|(part, ..)| *part);
    let count = parts.first()?.1;
    if parts.len() as u32 != count {
        return None;
    }
    let cover = parts[0].2.clone();
    for (i, (part, part_count, part_cover, _)) in parts.iter().enumerate() {
        if *part != i as u32 || *part_count != count || *part_cover != cover {
            return None;
        }
    }
    Some(StagedBaseline {
        cover,
        parts: parts.into_iter().map(|(_, _, _, body)| body).collect(),
    })
}

/// A parsed incoming event plus the highest schema version tag it carries
/// (visible even when we can't parse that schema).
struct StagedEvent {
    pos: EventPosition,
    event: Event,
    max_schema: SchemaVersion,
}

impl SyncEngine for EventSyncEngine {
    fn collection_name(&self) -> CollectionName {
        self.collection.clone()
    }

    fn stage_incoming(
        &self,
        mut inbound: Vec<IncomingBso>,
        _telem: &mut sync15::telemetry::Engine,
    ) -> anyhow::Result<()> {
        self.staged.lock().unwrap().append(&mut inbound);
        Ok(())
    }

    fn apply(
        &self,
        timestamp: ServerTimestamp,
        _telem: &mut sync15::telemetry::Engine,
    ) -> anyhow::Result<Vec<OutgoingBso>> {
        let staged = std::mem::take(&mut *self.staged.lock().unwrap());

        // Partition incoming records into events, baseline parts and markers.
        let mut events: Vec<StagedEvent> = Vec::new();
        // Baseline parts bucketed by group id; assembled into complete baselines below.
        let mut baseline_parts: HashMap<Guid, Vec<(u32, u32, EventPosition, EventBody)>> =
            HashMap::new();
        let mut baseline_needed = false;
        // Highest schema advertised by a peer's `baseline-needed` this sync; used
        // to elect the writer once the bar stays blocked past the timeout.
        let mut peer_needed_schema: Option<SchemaVersion> = None;
        for bso in staged {
            let pos = EventPosition::new(bso.envelope.modified, bso.envelope.id.clone());
            match bso.into_content::<WireRecord>().kind {
                IncomingKind::Content(WireRecord::Event {
                    event_type,
                    entity,
                    body,
                }) => {
                    let max_schema = body
                        .versions()
                        .max()
                        .unwrap_or_else(|| self.schema.current());
                    events.push(StagedEvent {
                        pos,
                        event: Event::new(event_type, entity, body),
                        max_schema,
                    });
                }
                IncomingKind::Content(WireRecord::Baseline {
                    group,
                    part,
                    part_count,
                    covers_up_to,
                    body,
                }) => baseline_parts.entry(group).or_default().push((
                    part,
                    part_count,
                    covers_up_to.into(),
                    body,
                )),
                IncomingKind::Content(WireRecord::BaselineNeeded { schema }) => {
                    baseline_needed = true;
                    peer_needed_schema = Some(peer_needed_schema.map_or(schema, |m| m.max(schema)));
                }
                // Events are immutable; tombstones/malformed are unexpected.
                _ => {}
            }
        }

        // Only complete groups count; a partial baseline is ignored until whole.
        let baselines: Vec<StagedBaseline> = baseline_parts
            .into_values()
            .filter_map(assemble_baseline)
            .collect();

        let mut state = self.meta.baseline_state()?;
        let is_new_client = self.meta.last_sync()?.as_millis() == 0 && state.floor.is_none();

        // 1. Received baselines: a new client cold-starts from the newest
        //    readable one; any client advances its floor to the newest cover
        //    (always parseable) and prunes.
        let mut floor_advance: Option<EventPosition> = None;
        if is_new_client {
            let mut readable: Vec<(EventPosition, Vec<SnapshotEntity>)> = Vec::new();
            for baseline in &baselines {
                if let Some(entities) = self.read_snapshot_parts(&baseline.parts)? {
                    readable.push((baseline.cover.clone(), entities));
                }
            }
            readable.sort_by(|a, b| a.0.cmp(&b.0));
            if let Some((cover, entities)) = readable.pop() {
                self.reducer.apply_baseline(entities);
                floor_advance = Some(cover);
            }
            // No readable baseline => apply all events from empty (floor stays
            // None). Present-but-unreadable is the below-floor case.
        } else if let Some(newest_cover) = baselines.iter().map(|b| b.cover.clone()).max() {
            if state
                .floor
                .as_ref()
                .is_none_or(|f| newest_cover.supersedes(f))
            {
                floor_advance = Some(newest_cover);
            }
        }
        if let Some(cover) = floor_advance {
            self.meta.prune_positions(&cover)?;
            state.newest_event = Some(max_position(state.newest_event.take(), cover.clone()));
            state.floor = Some(cover);
            state.events_since_baseline = 0;
            state.max_schema_since_baseline = self.schema.current();
            state.bar_blocked_since = None;
        }

        // Gate keys with a queued-but-unuploaded local edit. That edit is
        // already applied locally and will upload with a newer timestamp, so it
        // tentatively wins: don't let an incoming event for the same key clobber
        // it, or state and gate would disagree and clients diverge.
        let pending_keys: HashSet<GateKey> = self
            .outgoing
            .lock()
            .unwrap()
            .iter()
            .map(|p| p.gate.clone())
            .collect();

        // 2. Apply events newer than the floor in ascending order, accruing the
        //    cadence counters for events beyond the floor.
        events.sort_by(|a, b| a.pos.cmp(&b.pos));
        let floor = state.floor.clone();
        for staged_event in events {
            let pos = staged_event.pos.clone();
            let beyond_floor = floor.as_ref().is_none_or(|f| pos.supersedes(f));
            if beyond_floor {
                state.events_since_baseline += 1;
                state.max_schema_since_baseline =
                    state.max_schema_since_baseline.max(staged_event.max_schema);
                state.newest_event = Some(max_position(state.newest_event.take(), pos.clone()));
            }
            if pending_keys.contains(&staged_event.event.gate_key()) {
                continue;
            }
            self.ingest_with_floor(staged_event.pos, staged_event.event, floor.as_ref())?;
        }

        self.meta.set_last_sync(timestamp)?;

        // 3. Emit local events, remembered for `set_uploaded`.
        let mut bsos = Vec::new();
        let outgoing = std::mem::take(&mut *self.outgoing.lock().unwrap());
        {
            let mut awaiting = self.awaiting_upload.lock().unwrap();
            for p in outgoing {
                awaiting.insert(p.bso.envelope.id.clone(), p.gate);
                bsos.push(p.bso);
            }
        }

        // 4. Baseline cadence: once enough events pile up (or on `baseline-needed`),
        //    write a baseline if we meet the schema bar, else ask a capable client.
        let threshold_hit = self
            .baseline_config
            .events_per_baseline
            .is_some_and(|t| state.events_since_baseline >= t);
        if threshold_hit || baseline_needed {
            // Only meaningful if we've seen events beyond the floor.
            let has_new = state
                .newest_event
                .as_ref()
                .is_some_and(|n| state.floor.as_ref().is_none_or(|f| n.supersedes(f)));
            if has_new {
                let cover = state.newest_event.clone().unwrap();
                let meets_bar = self.schema.current() >= state.max_schema_since_baseline;

                // Timeout override: after the bar has stayed blocked past the timeout, the
                // highest-schema client still active writes anyway, dropping events
                // above its schema. Only peers that advertised `baseline-needed`
                // count; a departed author leaves no marker, so it can't outrank us.
                let peer_outranks = peer_needed_schema.is_some_and(|p| p > self.schema.current());
                let timed_out = matches!(
                    (self.baseline_config.baseline_timeout_ms, state.bar_blocked_since),
                    (Some(t), Some(since)) if timestamp.as_millis() - since.as_millis() >= t
                );

                if meets_bar || (timed_out && !peer_outranks) {
                    // Send the parts now. We wait for `set_uploaded` to confirm
                    // every part before advancing the floor and pruning, so a
                    // half-uploaded baseline never moves the floor.
                    let (group, parts) = self.build_baseline_bsos(&cover)?;
                    let remaining = parts.iter().map(|b| b.envelope.id.clone()).collect();
                    self.pending_baselines
                        .lock()
                        .unwrap()
                        .insert(group, PendingBaseline { cover, remaining });
                    bsos.extend(parts);
                } else {
                    // Below the bar: start the timeout clock, defer to a capable client.
                    if state.bar_blocked_since.is_none() {
                        state.bar_blocked_since = Some(timestamp);
                    }
                    if threshold_hit {
                        bsos.push(self.build_baseline_needed_bso()?);
                    }
                }
            }
        }

        self.meta.set_baseline_state(state)?;
        Ok(bsos)
    }

    fn set_uploaded(&self, new_timestamp: ServerTimestamp, ids: Vec<Guid>) -> anyhow::Result<()> {
        let mut awaiting = self.awaiting_upload.lock().unwrap();
        let mut pending = self.pending_baselines.lock().unwrap();
        let mut state = self.meta.baseline_state()?;
        for id in ids {
            if let Some(gate) = awaiting.remove(&id) {
                let pos = EventPosition::new(new_timestamp, id);
                self.meta.set_position(gate, pos.clone())?;
                // Count our own events toward the cadence: we never re-download
                // them, so this is the only place we observe them.
                if state.floor.as_ref().is_none_or(|f| pos.supersedes(f)) {
                    state.events_since_baseline += 1;
                    state.max_schema_since_baseline =
                        state.max_schema_since_baseline.max(self.schema.current());
                    state.newest_event = Some(max_position(state.newest_event.take(), pos));
                }
            } else {
                // Maybe a baseline part; mark it confirmed (ids are unique to
                // one group).
                for pb in pending.values_mut() {
                    pb.remaining.remove(&id);
                }
            }
        }

        // Once every part of a baseline is confirmed, do the floor advance and
        // prune the write path left for us.
        let complete: Vec<Guid> = pending
            .iter()
            .filter(|(_, pb)| pb.remaining.is_empty())
            .map(|(group, _)| group.clone())
            .collect();
        for group in complete {
            let pb = pending.remove(&group).expect("just listed");
            if state.floor.as_ref().is_none_or(|f| pb.cover.supersedes(f)) {
                self.meta.prune_positions(&pb.cover)?;
                state.newest_event =
                    Some(max_position(state.newest_event.take(), pb.cover.clone()));
                state.floor = Some(pb.cover);
                state.events_since_baseline = 0;
                state.max_schema_since_baseline = self.schema.current();
                state.bar_blocked_since = None;
            }
        }

        self.meta.set_baseline_state(state)?;
        self.meta.set_last_sync(new_timestamp)?;
        Ok(())
    }

    fn get_collection_request(
        &self,
        _server_timestamp: ServerTimestamp,
    ) -> anyhow::Result<Option<CollectionRequest>> {
        let last_sync = self.meta.last_sync()?;
        Ok(Some(
            CollectionRequest::new(self.collection.clone())
                .full()
                .newer_than(last_sync),
        ))
    }

    fn get_sync_assoc(&self) -> anyhow::Result<EngineSyncAssociation> {
        Ok(self.meta.sync_assoc()?)
    }

    fn reset(&self, assoc: &EngineSyncAssociation) -> anyhow::Result<()> {
        self.meta.reset()?;
        self.meta.set_sync_assoc(assoc.clone())?;
        Ok(())
    }

    fn wipe(&self) -> anyhow::Result<()> {
        // TODO: erasing local user data needs a reducer hook; for now we only
        // drop the engine's own sync metadata.
        self.meta.reset()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::BaselineConfig;
    use crate::metadata::MemoryMetadataStore;
    use crate::reducer::SnapshotEntity;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;

    /// A reducer that records what it was handed and can serve a snapshot.
    #[derive(Default)]
    struct RecordingReducer {
        applied: StdMutex<Vec<ResolvedEvent>>,
        baselines: StdMutex<Vec<Vec<SnapshotEntity>>>,
        snapshot: StdMutex<Vec<SnapshotEntity>>,
    }

    impl EventReducer for Arc<RecordingReducer> {
        fn snapshot(&self) -> Vec<SnapshotEntity> {
            self.snapshot.lock().unwrap().clone()
        }
        fn apply_baseline(&self, entities: Vec<SnapshotEntity>) {
            self.baselines.lock().unwrap().push(entities);
        }
        fn apply_event(&self, event: ResolvedEvent) {
            self.applied.lock().unwrap().push(event);
        }
    }

    fn engine_with(reducer: Arc<RecordingReducer>) -> EventSyncEngine {
        EventSyncEngine::new(
            "containers",
            Box::new(reducer),
            SchemaRegistry::new(1),
            Box::new(MemoryMetadataStore::new()),
        )
    }

    fn body(payload: serde_json::Value) -> EventBody {
        EventBody::single(1, payload)
    }

    /// An incoming BSO for a wire record at a given server position.
    fn incoming(record: &WireRecord, modified_ms: i64, id: &str) -> IncomingBso {
        OutgoingBso::from_content(OutgoingEnvelope::from(Guid::new(id)), record)
            .unwrap()
            .to_test_incoming_ts(ServerTimestamp::from_millis(modified_ms))
    }

    fn event_record(event_type: &str, entity: &str, payload: serde_json::Value) -> WireRecord {
        WireRecord::Event {
            event_type: event_type.to_string(),
            entity: Guid::new(entity),
            body: body(payload),
        }
    }

    #[test]
    fn newer_event_wins_older_is_ignored() {
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer.clone());
        let entity = Guid::new("containerX");

        assert!(engine
            .ingest_event(
                Guid::new("evt-bar"),
                ServerTimestamp::from_millis(210),
                Event::new(
                    "modify-name",
                    entity.clone(),
                    body(json!({ "name": "bar" }))
                ),
            )
            .unwrap());
        assert!(!engine
            .ingest_event(
                Guid::new("evt-foo"),
                ServerTimestamp::from_millis(200),
                Event::new(
                    "modify-name",
                    entity.clone(),
                    body(json!({ "name": "foo" }))
                ),
            )
            .unwrap());

        let applied = reducer.applied.lock().unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].payload, json!({ "name": "bar" }));
    }

    #[test]
    fn different_fields_do_not_gate_each_other() {
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer.clone());
        let entity = Guid::new("containerX");

        engine
            .ingest_event(
                Guid::new("evt-name"),
                ServerTimestamp::from_millis(210),
                Event::new(
                    "modify-name",
                    entity.clone(),
                    body(json!({ "name": "bar" })),
                ),
            )
            .unwrap();
        assert!(engine
            .ingest_event(
                Guid::new("evt-color"),
                ServerTimestamp::from_millis(200),
                Event::new(
                    "modify-color",
                    entity.clone(),
                    body(json!({ "color": "blue" }))
                ),
            )
            .unwrap());
        assert_eq!(reducer.applied.lock().unwrap().len(), 2);
    }

    #[test]
    fn new_client_cold_starts_from_baseline_then_applies_newer_events() {
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer.clone());

        // A baseline covering up to (100, "e100"), plus an older event (covered,
        // ignored) and a newer event (applied).
        let snapshot = vec![SnapshotEntity {
            entity: Guid::new("containerX"),
            payload: json!({ "name": "foo" }),
        }];
        let baseline = WireRecord::Baseline {
            group: Guid::new("g1"),
            part: 0,
            part_count: 1,
            covers_up_to: WirePosition {
                modified_ms: 100,
                record_id: Guid::new("e100"),
            },
            body: EventBody::single(1, serde_json::to_value(&snapshot).unwrap()),
        };

        let staged = vec![
            incoming(&baseline, 150, "baseline-1"),
            // Older than the cover -> ignored.
            incoming(
                &event_record("modify-name", "containerX", json!({ "name": "STALE" })),
                90,
                "e090",
            ),
            // Newer than the cover -> applied.
            incoming(
                &event_record("modify-name", "containerX", json!({ "name": "fresh" })),
                110,
                "e110",
            ),
        ];

        let mut telem = sync15::telemetry::Engine::new("containers");
        engine.stage_incoming(staged, &mut telem).unwrap();
        let outgoing = engine
            .apply(ServerTimestamp::from_millis(150), &mut telem)
            .unwrap();
        assert!(outgoing.is_empty());

        // Baseline applied once.
        let baselines = reducer.baselines.lock().unwrap();
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0][0].payload, json!({ "name": "foo" }));

        // Stale event ignored, fresh one applied.
        let applied = reducer.applied.lock().unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].payload, json!({ "name": "fresh" }));
    }

    #[test]
    fn cadence_writes_a_baseline_and_advances_the_floor() {
        let reducer = Arc::new(RecordingReducer::default());
        *reducer.snapshot.lock().unwrap() = vec![SnapshotEntity {
            entity: Guid::new("containerX"),
            payload: json!({ "name": "foo" }),
        }];
        // Threshold of 2 events triggers a baseline.
        let engine = engine_with(reducer.clone()).with_baseline_config(BaselineConfig {
            events_per_baseline: Some(2),
            baseline_timeout_ms: None,
            ..BaselineConfig::default()
        });

        let staged = vec![
            incoming(
                &event_record("created", "containerX", json!({ "name": "foo" })),
                100,
                "e100",
            ),
            incoming(
                &event_record("modify-name", "containerX", json!({ "name": "bar" })),
                110,
                "e110",
            ),
        ];
        let mut telem = sync15::telemetry::Engine::new("containers");
        engine.stage_incoming(staged, &mut telem).unwrap();
        let outgoing = engine
            .apply(ServerTimestamp::from_millis(110), &mut telem)
            .unwrap();

        // Exactly one baseline record emitted.
        assert_eq!(outgoing.len(), 1);
        let record: WireRecord = outgoing[0]
            .to_test_incoming()
            .into_content()
            .content()
            .unwrap();
        match record {
            WireRecord::Baseline { covers_up_to, .. } => {
                assert_eq!(covers_up_to.modified_ms, 110);
                assert_eq!(covers_up_to.record_id.as_str(), "e110");
            }
            _ => panic!("expected a baseline record"),
        }
    }

    #[test]
    fn client_below_schema_bar_emits_baseline_needed() {
        // Schema-1 client observing an event authored at schema 2.
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer).with_baseline_config(BaselineConfig {
            events_per_baseline: Some(1),
            baseline_timeout_ms: None,
            ..BaselineConfig::default()
        });

        // Body carries a schema-2 representation our schema-1 client sees but
        // can't parse.
        let mut evt_body = EventBody::single(1, json!({ "n": "foo" }));
        evt_body.insert(2, json!({ "name": "foo" }));
        let record = WireRecord::Event {
            event_type: "modify-name".to_string(),
            entity: Guid::new("containerX"),
            body: evt_body,
        };

        let mut telem = sync15::telemetry::Engine::new("containers");
        engine
            .stage_incoming(vec![incoming(&record, 100, "e100")], &mut telem)
            .unwrap();
        let outgoing = engine
            .apply(ServerTimestamp::from_millis(100), &mut telem)
            .unwrap();

        assert_eq!(outgoing.len(), 1);
        let parsed: WireRecord = outgoing[0]
            .to_test_incoming()
            .into_content()
            .content()
            .unwrap();
        assert!(matches!(parsed, WireRecord::BaselineNeeded { schema: 1 }));
    }

    fn baseline_part(group: &str, part: u32, count: u32, ent: &str, name: &str) -> WireRecord {
        let snapshot = vec![SnapshotEntity {
            entity: Guid::new(ent),
            payload: json!({ "name": name }),
        }];
        WireRecord::Baseline {
            group: Guid::new(group),
            part,
            part_count: count,
            covers_up_to: WirePosition {
                modified_ms: 100,
                record_id: Guid::new("e100"),
            },
            body: EventBody::single(1, serde_json::to_value(&snapshot).unwrap()),
        }
    }

    #[test]
    fn assemble_baseline_orders_parts_numerically() {
        let cover = EventPosition::new(ServerTimestamp::from_millis(100), Guid::new("cover"));
        let count = 11;
        // Bodies tagged with their part index, supplied in reverse so `_10`
        // precedes `_2` under a naive sort.
        let mut parts: Vec<(u32, u32, EventPosition, EventBody)> = (0..count)
            .map(|i| (i, count, cover.clone(), EventBody::single(1, json!(i))))
            .collect();
        parts.reverse();

        let assembled = assemble_baseline(parts).expect("complete group");
        assert_eq!(assembled.cover, cover);
        let order: Vec<u32> = assembled
            .parts
            .iter()
            .map(|b| serde_json::from_value(b.get(1).unwrap().clone()).unwrap())
            .collect();
        assert_eq!(order, (0..count).collect::<Vec<_>>());
    }

    #[test]
    fn assemble_baseline_rejects_incomplete_group() {
        let cover = EventPosition::new(ServerTimestamp::from_millis(100), Guid::new("cover"));
        // Claims 3 parts but only 2 are present.
        let parts = vec![
            (0, 3, cover.clone(), EventBody::single(1, json!(0))),
            (1, 3, cover.clone(), EventBody::single(1, json!(1))),
        ];
        assert!(assemble_baseline(parts).is_none());
    }

    #[test]
    fn new_client_cold_starts_from_a_split_baseline() {
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer.clone());

        // Two parts of one baseline, delivered out of order.
        let staged = vec![
            incoming(&baseline_part("g", 1, 2, "B", "b"), 150, "g_1"),
            incoming(&baseline_part("g", 0, 2, "A", "a"), 150, "g_0"),
        ];
        let mut telem = sync15::telemetry::Engine::new("containers");
        engine.stage_incoming(staged, &mut telem).unwrap();
        engine
            .apply(ServerTimestamp::from_millis(150), &mut telem)
            .unwrap();

        let baselines = reducer.baselines.lock().unwrap();
        assert_eq!(baselines.len(), 1);
        // Reassembled in part order, not delivery order.
        let names: Vec<String> = baselines[0]
            .iter()
            .map(|e| e.payload["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn incomplete_baseline_group_is_ignored() {
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer.clone());

        // Only part 0 of a 2-part group arrives.
        let mut telem = sync15::telemetry::Engine::new("containers");
        engine
            .stage_incoming(
                vec![incoming(&baseline_part("g", 0, 2, "A", "a"), 150, "g_0")],
                &mut telem,
            )
            .unwrap();
        engine
            .apply(ServerTimestamp::from_millis(150), &mut telem)
            .unwrap();

        // The partial group is not applied.
        assert!(reducer.baselines.lock().unwrap().is_empty());
    }

    #[test]
    fn floor_advances_only_after_baseline_upload_confirmed() {
        let reducer = Arc::new(RecordingReducer::default());
        *reducer.snapshot.lock().unwrap() = vec![SnapshotEntity {
            entity: Guid::new("X"),
            payload: json!({ "name": "x" }),
        }];
        let engine = engine_with(reducer.clone()).with_baseline_config(BaselineConfig {
            events_per_baseline: Some(1),
            baseline_timeout_ms: None,
            max_record_payload_bytes: usize::MAX,
        });
        let mut telem = sync15::telemetry::Engine::new("containers");

        // An incoming event trips the cadence; a baseline is emitted but not yet
        // confirmed uploaded.
        engine
            .stage_incoming(
                vec![incoming(
                    &event_record("created", "X", json!({ "name": "x" })),
                    100,
                    "e100",
                )],
                &mut telem,
            )
            .unwrap();
        let outgoing = engine
            .apply(ServerTimestamp::from_millis(100), &mut telem)
            .unwrap();
        let baseline_id = outgoing[0].envelope.id.clone();

        // Upload unconfirmed => floor unset => an event below the cover applies.
        engine
            .stage_incoming(
                vec![incoming(
                    &event_record("created", "below", json!({ "name": "below" })),
                    90,
                    "e090",
                )],
                &mut telem,
            )
            .unwrap();
        engine
            .apply(ServerTimestamp::from_millis(150), &mut telem)
            .unwrap();
        assert!(
            reducer
                .applied
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.payload["name"] == json!("below")),
            "event below an unconfirmed baseline should still apply"
        );

        // Confirm the upload: the floor now advances to the cover.
        engine
            .set_uploaded(ServerTimestamp::from_millis(120), vec![baseline_id])
            .unwrap();

        // A later event below the cover is now pruned by the floor.
        engine
            .stage_incoming(
                vec![incoming(
                    &event_record("created", "stale", json!({ "name": "stale" })),
                    95,
                    "e095",
                )],
                &mut telem,
            )
            .unwrap();
        engine
            .apply(ServerTimestamp::from_millis(160), &mut telem)
            .unwrap();
        assert!(
            !reducer
                .applied
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.payload["name"] == json!("stale")),
            "event below a confirmed baseline's floor should be ignored"
        );
    }
}
