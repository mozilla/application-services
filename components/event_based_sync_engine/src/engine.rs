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

use crate::baseline::BaselineConfig;
use crate::error::{Error, Result};
use crate::event::{
    batch_record_id, EventPosition, GateKey, IncomingEvent, Wire, WireEvent, WirePart,
    BASELINE_EVENT_TYPE, BASELINE_NEEDED_EVENT_TYPE,
};
use crate::metadata::MetadataStore;
use crate::reducer::{EventReducer, ResolvedEvent};
use crate::schema::{Body, SchemaRegistry, SchemaVersion};
use base64::prelude::{Engine, BASE64_STANDARD};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use sync15::bso::{IncomingBso, IncomingKind, OutgoingBso, OutgoingEnvelope};
use sync15::engine::{CollectionRequest, EngineSyncAssociation, SyncEngine};
use sync15::{CollectionName, Guid, ServerTimestamp};

/// Bytes reserved for a [`WirePart`]'s envelope fields (`group`, `part`,
/// `part_count`, field names, quoting) when sizing the `data` slice. Generous
/// next to any real budget.
const FRAME_OVERHEAD: usize = 96;

/// Default byte budget for a single record before it is framed into `_N` parts.
pub const DEFAULT_MAX_RECORD_PAYLOAD_BYTES: usize = 256 * 1024;

/// Split `s`'s raw bytes into base64-encoded frame payloads.
fn split_frames(s: &str, max_bytes: usize) -> Vec<String> {
    // base64 emits 4 output chars per 3 input bytes
    let chunk = (max_bytes.max(4) / 4) * 3;
    s.as_bytes()
        .chunks(chunk)
        .map(|c| BASE64_STANDARD.encode(c))
        .collect()
}

/// A local event awaiting upload, remembered so [`SyncEngine::set_uploaded`] can
/// record its final server position into the gate. `position_id` is the id the
/// event's [`EventPosition`] is keyed on (its record id, or its group when the
/// record was framed).
struct PendingOutgoing {
    gate: GateKey,
    position_id: Guid,
    bsos: Vec<OutgoingBso>,
}

/// A record we uploaded, keyed by group id, waiting for
/// [`SyncEngine::set_uploaded`] to confirm each of its frames. Its
/// [`OnComplete`] runs only once every frame is confirmed, so a half-uploaded
/// record (its frames may span sync batches) never moves a gate or the floor.
/// Events and baselines share this and differ only in `on_complete`.
struct AwaitingConfirmation {
    remaining: HashSet<Guid>,
    on_complete: OnComplete,
}

/// What confirming an [`AwaitingConfirmation`]'s last frame does. An event
/// extends the stream (advances its gate, counts toward the cadence); a baseline
/// collapses it (advances the floor, prunes below it, resets the cadence).
enum OnComplete {
    /// `position_id` is the id the event's [`EventPosition`] is keyed on (its
    /// group id).
    Event {
        gate: GateKey,
        position_id: Guid,
    },
    Baseline {
        cover: EventPosition,
    },
}

/// A record read from the server, tagged with the group its frames reassemble
/// into (its own record id when whole) and its record position.
struct StagedRecord {
    group: Guid,
    modified: ServerTimestamp,
    raw: RawRecord,
}

/// Either a complete record or one frame of a split one.
enum RawRecord {
    Whole(WireEvent),
    Frame {
        part: u32,
        part_count: u32,
        data: String,
    },
}

pub struct EventSyncEngine {
    collection: CollectionName,
    reducer: Box<dyn EventReducer>,
    schema: SchemaRegistry,
    meta: Box<dyn MetadataStore>,
    baseline_config: BaselineConfig,
    /// Byte budget for a single record. A larger record is split into `_N`
    /// parts, each under the budget, by slicing its serialized bytes (the
    /// payload is never inspected). Applies to
    /// every event.
    max_record_payload_bytes: usize,

    /// Incoming records staged by `stage_incoming`, drained by `apply`.
    staged: Mutex<Vec<IncomingBso>>,
    /// Local events recorded but not yet handed to `apply`.
    outgoing: Mutex<Vec<PendingOutgoing>>,
    /// Framed records we uploaded (events and baselines alike), keyed by group
    /// id, awaiting confirmation of every frame before their [`OnComplete`]
    /// side-effect runs.
    awaiting_confirmation: Mutex<HashMap<Guid, AwaitingConfirmation>>,
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
            max_record_payload_bytes: DEFAULT_MAX_RECORD_PAYLOAD_BYTES,
            staged: Mutex::new(Vec::new()),
            outgoing: Mutex::new(Vec::new()),
            awaiting_confirmation: Mutex::new(HashMap::new()),
        }
    }

    /// Override the baseline cadence configuration.
    pub fn with_baseline_config(mut self, config: BaselineConfig) -> Self {
        self.baseline_config = config;
        self
    }

    /// Override the per-record byte budget used to frame every event.
    pub fn with_max_record_payload_bytes(mut self, max: usize) -> Self {
        self.max_record_payload_bytes = max;
        self
    }

    /// Record a local change as an event to upload on the next sync.
    ///
    /// `payload` is in the client's current schema; the engine writes it in
    /// every schema down to the floor. The gate advances in `set_uploaded`, once the server
    /// assigns the authoritative `modified` timestamp.
    pub fn record_local_event(
        &self,
        event_type: &str,
        entity: Guid,
        payload: serde_json::Value,
    ) -> Result<()> {
        let body = self.schema.write(event_type, payload)?;
        let record = WireEvent::event(event_type.to_string(), entity.clone(), body);
        let base = Guid::random();
        let bsos = self.frame(&record, &base)?;
        let gate = GateKey::new(event_type, entity);
        let mut outgoing = self.outgoing.lock().unwrap();
        // One batch shares a `modified`, so two queued events for the same key
        // would tie and the random record id, not edit order, would pick the
        // winner. Only the newest can win anyway: replace the older one.
        outgoing.retain(|p| p.gate != gate);
        outgoing.push(PendingOutgoing {
            gate,
            position_id: base,
            bsos,
        });
        Ok(())
    }

    /// Serialize `record` into upload BSOs. If it fits the record budget it ships
    /// whole under `base`; otherwise its bytes are split and base64-encoded into
    /// `base_0 ..` [`WirePart`] frames. The engine never inspects the payload.
    fn frame(&self, record: &WireEvent, base: &Guid) -> Result<Vec<OutgoingBso>> {
        let json = serde_json::to_string(record)?;
        let max = self.max_record_payload_bytes;
        if json.len() <= max {
            // Reuse the JSON we already serialized to measure. Like a frame, it
            // carries no `id`; the reader re-adds it from the envelope.
            return Ok(vec![OutgoingBso {
                envelope: OutgoingEnvelope::from(base.clone()),
                payload: json,
            }]);
        }
        // Each frame's base64 `data` is plain ASCII JSON never re-escapes, so the
        // whole envelope-adjusted budget is available for it.
        let data_budget = max.saturating_sub(FRAME_OVERHEAD).max(1);
        let chunks = split_frames(&json, data_budget);
        let part_count = chunks.len() as u32;
        let mut bsos = Vec::with_capacity(chunks.len());
        for (i, data) in chunks.into_iter().enumerate() {
            let frame = Wire::Frame(WirePart {
                group: base.clone(),
                part: i as u32,
                part_count,
                data,
            });
            let id = batch_record_id(base, i);
            bsos.push(OutgoingBso::from_content(
                OutgoingEnvelope::from(id),
                &frame,
            )?);
        }
        Ok(bsos)
    }

    /// Gate + dispatch an assembled event against the baseline floor. Returns
    /// whether it won and was dispatched to the reducer.
    ///
    /// Feed events in ascending server order so the gate only moves forward;
    /// [`EventSyncEngine::apply`] does this.
    fn ingest(&self, ev: &IncomingEvent, floor: Option<&EventPosition>) -> Result<bool> {
        // Ignore anything superseded by a baseline.
        if let Some(floor) = floor {
            if !ev.position.supersedes(floor) {
                return Ok(false);
            }
        }

        let gate = &ev.gate;
        if let Some(stored) = self.meta.position(gate)? {
            if !ev.position.supersedes(&stored) {
                // Older than or identical to what we already applied.
                return Ok(false);
            }
        }

        // Migrate-on-read to our schema. An unreadable event is below our floor:
        // skip it without advancing the gate.
        let payload = match self
            .schema
            .read(&gate.event_type, &ev.body, self.schema.current())
        {
            Ok(payload) => payload,
            Err(Error::NoReadableSchema { .. }) => return Ok(false),
            Err(e) => return Err(e),
        };

        self.reducer.apply_event(ResolvedEvent {
            event_type: gate.event_type.clone(),
            entity: gate.entity.clone(),
            payload,
        });
        self.meta.set_position(gate.clone(), ev.position.clone())?;
        Ok(true)
    }

    /// Resolve a baseline's body to the current schema. `None` if it carries no
    /// schema this client can read (the baseline is then below our floor).
    fn read_baseline(&self, body: &Body) -> Result<Option<serde_json::Value>> {
        match self
            .schema
            .read(BASELINE_EVENT_TYPE, body, self.schema.current())
        {
            Ok(payload) => Ok(Some(payload)),
            Err(Error::NoReadableSchema { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Serialize the reducer's current state into a baseline record, framed if it
    /// exceeds the record budget. Returns the group id and its frames.
    fn build_baseline_bsos(
        &self,
        covers_up_to: &EventPosition,
    ) -> Result<(Guid, Vec<OutgoingBso>)> {
        let payload = self.reducer.baseline();
        let body = self.schema.write(BASELINE_EVENT_TYPE, payload)?;
        let record = WireEvent::baseline(covers_up_to.clone(), body);
        let group = Guid::random();
        let bsos = self.frame(&record, &group)?;
        Ok((group, bsos))
    }

    fn build_baseline_needed_bso(&self) -> Result<OutgoingBso> {
        Ok(OutgoingBso::from_content(
            OutgoingEnvelope::from(Guid::random()),
            WireEvent::baseline_needed(self.schema.current()),
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

/// Parse each staged BSO into a [`StagedRecord`], tagged with the group its
/// frames reassemble into (a whole record's group is its own id) and its record
/// position. Tombstones and malformed records are dropped.
fn staged_records(staged: Vec<IncomingBso>) -> Vec<StagedRecord> {
    let mut parts = Vec::with_capacity(staged.len());
    for bso in staged {
        let modified = bso.envelope.modified;
        let record_id = bso.envelope.id.clone();
        match bso.into_content::<Wire>().kind {
            IncomingKind::Content(Wire::Whole(wire)) => parts.push(StagedRecord {
                group: record_id,
                modified,
                raw: RawRecord::Whole(wire),
            }),
            IncomingKind::Content(Wire::Frame(p)) => parts.push(StagedRecord {
                group: p.group,
                modified,
                raw: RawRecord::Frame {
                    part: p.part,
                    part_count: p.part_count,
                    data: p.data,
                },
            }),
            _ => {}
        }
    }
    parts
}

/// Reassemble records into whole [`IncomingEvent`]s. Records are bucketed by
/// group: a whole record is a size-1 group used directly; a framed record's
/// frames are joined (in `part` order, base64-decoding `data`) into the original
/// serialized [`WireEvent`] and parsed once. A framed group is skipped until it
/// holds exactly `0..part_count`, so a baseline mid-upload is never half-applied.
///
/// `current` is the schema fallback for a record carrying no readable version
/// tag (a `baseline-needed` marker has no body).
fn assemble_events(parts: Vec<StagedRecord>, current: SchemaVersion) -> Vec<IncomingEvent> {
    let mut buckets: HashMap<Guid, Vec<StagedRecord>> = HashMap::new();
    for part in parts {
        buckets.entry(part.group.clone()).or_default().push(part);
    }

    let mut out = Vec::with_capacity(buckets.len());
    for (group, bucket) in buckets {
        // The record is complete once its newest frame lands, so key the position
        // off the max `modified` across the group — deterministic regardless of
        // frame arrival order, and matching the writer's own gate position.
        let modified = bucket
            .iter()
            .map(|r| r.modified)
            .max_by_key(|t| t.as_millis())
            .expect("bucket is non-empty");
        let Some(wire) = reassemble(bucket) else {
            continue;
        };
        let max_schema = wire
            .body
            .versions()
            .max()
            .or(wire.schema)
            .unwrap_or(current);
        out.push(IncomingEvent {
            gate: GateKey::new(wire.event_type, wire.entity),
            // Key the position off the group so it is stable regardless of which
            // frame we read it from.
            position: EventPosition::new(modified, group),
            body: wire.body,
            max_schema,
            covers_up_to: wire.covers_up_to,
        });
    }
    out
}

/// Join one group's staged records into the original [`WireEvent`], or `None` if
/// the group is an incomplete/inconsistent set of frames or fails to parse.
fn reassemble(mut bucket: Vec<StagedRecord>) -> Option<WireEvent> {
    // A lone whole record stands alone and is used directly; a lone frame or a
    // larger group falls through to frame reassembly below.
    if let [StagedRecord {
        raw: RawRecord::Whole(_),
        ..
    }] = bucket.as_slice()
    {
        let RawRecord::Whole(wire) = bucket.remove(0).raw else {
            unreachable!("slice pattern guarantees a whole record")
        };
        return Some(wire);
    }

    // Otherwise it is a set of frames: sort by part, require exactly `0..count`,
    // then base64-decode and concatenate the data and parse the original record.
    let mut frames: Vec<(u32, u32, String)> = bucket
        .into_iter()
        .map(|r| match r.raw {
            RawRecord::Frame {
                part,
                part_count,
                data,
            } => Some((part, part_count, data)),
            RawRecord::Whole(_) => None,
        })
        .collect::<Option<_>>()?;
    frames.sort_by_key(|(part, ..)| *part);
    let count = frames.first()?.1;
    if frames.len() as u32 != count {
        return None;
    }
    let mut joined = Vec::new();
    for (i, (part, part_count, data)) in frames.into_iter().enumerate() {
        if part != i as u32 || part_count != count {
            return None;
        }
        joined.extend(BASE64_STANDARD.decode(&data).ok()?);
    }
    serde_json::from_slice(&joined).ok()
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

        // Parse -> reassemble parts -> a stream of whole events. Then split by
        // reserved type into domain events, baselines and `baseline-needed`
        // markers.
        let assembled = assemble_events(staged_records(staged), self.schema.current());
        let mut events: Vec<IncomingEvent> = Vec::new();
        let mut baselines: Vec<IncomingEvent> = Vec::new();
        let mut baseline_needed = false;
        // Highest schema advertised by a peer's `baseline-needed` this sync; used
        // to elect the writer once the bar stays blocked past the timeout.
        let mut peer_needed_schema: Option<SchemaVersion> = None;
        for ev in assembled {
            if ev.gate.event_type == BASELINE_EVENT_TYPE {
                baselines.push(ev);
            } else if ev.gate.event_type == BASELINE_NEEDED_EVENT_TYPE {
                baseline_needed = true;
                peer_needed_schema =
                    Some(peer_needed_schema.map_or(ev.max_schema, |m| m.max(ev.max_schema)));
            } else {
                events.push(ev);
            }
        }

        let mut state = self.meta.baseline_state()?;
        let is_new_client = self.meta.last_sync()?.as_millis() == 0 && state.floor.is_none();

        // 1. Received baselines: a new client cold-starts from the newest
        //    readable one; any client advances its floor to the newest cover
        //    (always parseable) and prunes.
        let mut floor_advance: Option<EventPosition> = None;
        if is_new_client {
            let mut readable: Vec<(EventPosition, serde_json::Value)> = Vec::new();
            for baseline in &baselines {
                let Some(cover) = baseline.covers_up_to.clone() else {
                    continue;
                };
                if let Some(payload) = self.read_baseline(&baseline.body)? {
                    readable.push((cover, payload));
                }
            }
            readable.sort_by(|a, b| a.0.cmp(&b.0));
            if let Some((cover, payload)) = readable.pop() {
                self.reducer.apply_baseline(payload);
                floor_advance = Some(cover);
            }
            // No readable baseline => apply all events from empty (floor stays
            // None). Present-but-unreadable is the below-floor case.
        } else if let Some(newest_cover) = baselines
            .iter()
            .filter_map(|b| b.covers_up_to.clone())
            .max()
        {
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
        events.sort_by(|a, b| a.position.cmp(&b.position));
        let floor = state.floor.clone();
        for ev in events {
            let pos = ev.position.clone();
            let beyond_floor = floor.as_ref().is_none_or(|f| pos.supersedes(f));
            if beyond_floor {
                state.events_since_baseline += 1;
                state.max_schema_since_baseline =
                    state.max_schema_since_baseline.max(ev.max_schema);
                state.newest_event = Some(max_position(state.newest_event.take(), pos.clone()));
            }
            if pending_keys.contains(&ev.gate) {
                continue;
            }
            self.ingest(&ev, floor.as_ref())?;
        }

        self.meta.set_last_sync(timestamp)?;

        // 3. Emit local events, remembered for `set_uploaded`. A framed event
        //    tracks every frame; the gate only advances once all are confirmed.
        let mut bsos = Vec::new();
        let outgoing = std::mem::take(&mut *self.outgoing.lock().unwrap());
        {
            let mut awaiting = self.awaiting_confirmation.lock().unwrap();
            for p in outgoing {
                let remaining = p.bsos.iter().map(|b| b.envelope.id.clone()).collect();
                awaiting.insert(
                    p.position_id.clone(),
                    AwaitingConfirmation {
                        remaining,
                        on_complete: OnComplete::Event {
                            gate: p.gate,
                            position_id: p.position_id,
                        },
                    },
                );
                bsos.extend(p.bsos);
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
                    self.awaiting_confirmation.lock().unwrap().insert(
                        group,
                        AwaitingConfirmation {
                            remaining,
                            on_complete: OnComplete::Baseline { cover },
                        },
                    );
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
        let mut awaiting = self.awaiting_confirmation.lock().unwrap();
        let mut state = self.meta.baseline_state()?;
        for id in ids {
            // Confirm the frame against its group (ids are unique to one group).
            for p in awaiting.values_mut() {
                if p.remaining.remove(&id) {
                    break;
                }
            }
        }

        // A record split across sync batches completes only on its final batch.
        // Run events before baselines: a baseline resets the cadence counter
        // an event increments, so the order must not depend on map iteration.
        let complete: Vec<Guid> = awaiting
            .iter()
            .filter(|(_, p)| p.remaining.is_empty())
            .map(|(group, _)| group.clone())
            .collect();
        let mut events = Vec::new();
        let mut baselines = Vec::new();
        for group in complete {
            match awaiting.remove(&group).expect("just listed").on_complete {
                OnComplete::Event { gate, position_id } => events.push((gate, position_id)),
                OnComplete::Baseline { cover } => baselines.push(cover),
            }
        }

        // Confirmed events advance the gate.
        for (gate, position_id) in events {
            // Key the gate off the event's position id (its group), not any one
            // frame's id, so a framed event advances the gate once.
            let pos = EventPosition::new(new_timestamp, position_id);
            self.meta.set_position(gate, pos.clone())?;
            // Count our own events toward the cadence: we never re-download them,
            // so this is the only place we observe them.
            if state.floor.as_ref().is_none_or(|f| pos.supersedes(f)) {
                state.events_since_baseline += 1;
                state.max_schema_since_baseline =
                    state.max_schema_since_baseline.max(self.schema.current());
                state.newest_event = Some(max_position(state.newest_event.take(), pos));
            }
        }

        // Confirmed baselines advance the floor and prune below it.
        for cover in baselines {
            if state.floor.as_ref().is_none_or(|f| cover.supersedes(f)) {
                self.meta.prune_positions(&cover)?;
                state.newest_event = Some(max_position(state.newest_event.take(), cover.clone()));
                state.floor = Some(cover);
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
    use serde::Serialize;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;

    /// A reducer that records what it was handed and can serve a baseline. Its
    /// baseline payload is an array of `{ entity, payload }` objects.
    #[derive(Default)]
    struct RecordingReducer {
        applied: StdMutex<Vec<ResolvedEvent>>,
        baselines: StdMutex<Vec<serde_json::Value>>,
        baseline: StdMutex<serde_json::Value>,
    }

    impl EventReducer for Arc<RecordingReducer> {
        fn baseline(&self) -> serde_json::Value {
            let b = self.baseline.lock().unwrap();
            if b.is_null() {
                json!([])
            } else {
                b.clone()
            }
        }
        fn apply_baseline(&self, payload: serde_json::Value) {
            self.baselines.lock().unwrap().push(payload);
        }
        fn apply_event(&self, event: ResolvedEvent) {
            self.applied.lock().unwrap().push(event);
        }
    }

    /// A baseline payload as the reducer produces it: an array of entities.
    fn baseline_payload(entities: &[(&str, serde_json::Value)]) -> serde_json::Value {
        serde_json::Value::Array(
            entities
                .iter()
                .map(|(id, payload)| json!({ "entity": id, "payload": payload }))
                .collect(),
        )
    }

    fn engine_with(reducer: Arc<RecordingReducer>) -> EventSyncEngine {
        EventSyncEngine::new(
            "containers",
            Box::new(reducer),
            SchemaRegistry::new(1),
            Box::new(MemoryMetadataStore::new()),
        )
    }

    fn body(payload: serde_json::Value) -> Body {
        Body::single(1, payload)
    }

    fn pos(millis: i64, id: &str) -> EventPosition {
        EventPosition::new(ServerTimestamp::from_millis(millis), Guid::new(id))
    }

    /// An incoming BSO for any wire record at a given server position.
    fn incoming<T: Serialize>(record: &T, modified_ms: i64, id: &str) -> IncomingBso {
        OutgoingBso::from_content(OutgoingEnvelope::from(Guid::new(id)), record)
            .unwrap()
            .to_test_incoming_ts(ServerTimestamp::from_millis(modified_ms))
    }

    /// Frame a baseline record into `_{n}` [`WirePart`] BSOs at `max_bytes`, as
    /// the engine would when it overflows the budget.
    fn baseline_frames(
        group: &str,
        cover_ms: i64,
        cover_id: &str,
        entities: &[(&str, serde_json::Value)],
        max_bytes: usize,
    ) -> Vec<IncomingBso> {
        let body = Body::single(1, baseline_payload(entities));
        let record = WireEvent::baseline(pos(cover_ms, cover_id), body);
        let json = serde_json::to_string(&record).unwrap();
        let chunks = split_frames(&json, max_bytes);
        let count = chunks.len() as u32;
        chunks
            .into_iter()
            .enumerate()
            .map(|(i, data)| {
                let frame = Wire::Frame(WirePart {
                    group: Guid::new(group),
                    part: i as u32,
                    part_count: count,
                    data,
                });
                let id = batch_record_id(&Guid::new(group), i);
                incoming(&frame, cover_ms + 50, id.as_str())
            })
            .collect()
    }

    fn event_record(event_type: &str, entity: &str, payload: serde_json::Value) -> WireEvent {
        WireEvent::event(event_type.to_string(), Guid::new(entity), body(payload))
    }

    /// An assembled domain event, for gating [`EventSyncEngine::ingest`].
    fn incoming_event(
        event_type: &str,
        entity: &str,
        modified_ms: i64,
        id: &str,
        payload: serde_json::Value,
    ) -> IncomingEvent {
        IncomingEvent {
            gate: GateKey::new(event_type, Guid::new(entity)),
            position: pos(modified_ms, id),
            body: body(payload),
            max_schema: 1,
            covers_up_to: None,
        }
    }

    #[test]
    fn domain_event_wire_is_lean() {
        // A domain event carries only type/entity/body: the baseline-only fields
        // (covers_up_to, schema) default and stay off the wire, and framing is a
        // separate shape, so nothing else appears.
        let wire = serde_json::to_value(event_record(
            "modify-name",
            "containerX",
            json!({ "name": "foo" }),
        ))
        .unwrap();
        let obj = wire.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["body", "entity", "type"]);
    }

    #[test]
    fn newer_event_wins_older_is_ignored() {
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer.clone());
        let entity = Guid::new("containerX");

        assert!(engine
            .ingest(
                &incoming_event(
                    "modify-name",
                    entity.as_str(),
                    210,
                    "evt-bar",
                    json!({ "name": "bar" })
                ),
                None,
            )
            .unwrap());
        assert!(!engine
            .ingest(
                &incoming_event(
                    "modify-name",
                    entity.as_str(),
                    200,
                    "evt-foo",
                    json!({ "name": "foo" })
                ),
                None,
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
            .ingest(
                &incoming_event(
                    "modify-name",
                    entity.as_str(),
                    210,
                    "evt-name",
                    json!({ "name": "bar" }),
                ),
                None,
            )
            .unwrap();
        assert!(engine
            .ingest(
                &incoming_event(
                    "modify-color",
                    entity.as_str(),
                    200,
                    "evt-color",
                    json!({ "color": "blue" })
                ),
                None,
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
        let baseline = WireEvent::baseline(
            pos(100, "e100"),
            Body::single(
                1,
                baseline_payload(&[("containerX", json!({ "name": "foo" }))]),
            ),
        );

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

        // Baseline applied once, as the reducer's own payload array.
        let baselines = reducer.baselines.lock().unwrap();
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0][0]["payload"], json!({ "name": "foo" }));

        // Stale event ignored, fresh one applied.
        let applied = reducer.applied.lock().unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].payload, json!({ "name": "fresh" }));
    }

    #[test]
    fn cadence_writes_a_baseline_and_advances_the_floor() {
        let reducer = Arc::new(RecordingReducer::default());
        *reducer.baseline.lock().unwrap() =
            baseline_payload(&[("containerX", json!({ "name": "foo" }))]);
        // Threshold of 2 events triggers a baseline.
        let engine = engine_with(reducer.clone()).with_baseline_config(BaselineConfig {
            events_per_baseline: Some(2),
            baseline_timeout_ms: None,
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
        let record: WireEvent = outgoing[0]
            .to_test_incoming()
            .into_content()
            .content()
            .unwrap();
        assert_eq!(record.event_type, BASELINE_EVENT_TYPE);
        let cover = record.covers_up_to.expect("a baseline carries a cover");
        assert_eq!(cover, pos(110, "e110"));
    }

    #[test]
    fn client_below_schema_bar_emits_baseline_needed() {
        // Schema-1 client observing an event authored at schema 2.
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer).with_baseline_config(BaselineConfig {
            events_per_baseline: Some(1),
            baseline_timeout_ms: None,
        });

        // Body carries a schema-2 representation our schema-1 client sees but
        // can't parse.
        let mut evt_body = Body::single(1, json!({ "n": "foo" }));
        evt_body.insert(2, json!({ "name": "foo" }));
        let record = WireEvent::event("modify-name".to_string(), Guid::new("containerX"), evt_body);

        let mut telem = sync15::telemetry::Engine::new("containers");
        engine
            .stage_incoming(vec![incoming(&record, 100, "e100")], &mut telem)
            .unwrap();
        let outgoing = engine
            .apply(ServerTimestamp::from_millis(100), &mut telem)
            .unwrap();

        assert_eq!(outgoing.len(), 1);
        let parsed: WireEvent = outgoing[0]
            .to_test_incoming()
            .into_content()
            .content()
            .unwrap();
        assert_eq!(parsed.event_type, BASELINE_NEEDED_EVENT_TYPE);
        assert_eq!(parsed.schema, Some(1));
    }

    /// Frame `record` into [`StagedRecord`] frames at `max_bytes`, for exercising
    /// [`assemble_events`] directly.
    fn frames_of(record: &WireEvent, group: &str, max_bytes: usize) -> Vec<StagedRecord> {
        let json = serde_json::to_string(record).unwrap();
        let chunks = split_frames(&json, max_bytes);
        let count = chunks.len() as u32;
        chunks
            .into_iter()
            .enumerate()
            .map(|(i, data)| StagedRecord {
                group: Guid::new(group),
                modified: ServerTimestamp::from_millis(100),
                raw: RawRecord::Frame {
                    part: i as u32,
                    part_count: count,
                    data,
                },
            })
            .collect()
    }

    #[test]
    fn assemble_events_reassembles_frames_in_numeric_order() {
        // A baseline sliced tiny, so it takes more than ten frames. That is enough
        // that a plain string sort (rather than numeric) would put `_10` before
        // `_2` and corrupt the reassembled bytes.
        let body = Body::single(
            1,
            baseline_payload(&[
                ("A", json!({ "x": 1 })),
                ("B", json!({ "x": 2 })),
                ("C", json!({ "x": 3 })),
                ("D", json!({ "x": 4 })),
            ]),
        );
        let record = WireEvent::baseline(pos(100, "cover"), body.clone());
        let mut frames = frames_of(&record, "g", 15);
        assert!(
            frames.len() > 10,
            "need >10 frames to exercise numeric ordering, got {}",
            frames.len()
        );
        frames.reverse();

        let assembled = assemble_events(frames, 1);
        assert_eq!(assembled.len(), 1);
        let ev = &assembled[0];
        assert_eq!(ev.covers_up_to, Some(pos(100, "cover")));
        // Frames reassembled to the identical body despite reversed delivery.
        assert_eq!(ev.body, body);
    }

    #[test]
    fn assemble_events_position_uses_max_modified_regardless_of_order() {
        // Frames of one record can carry different `modified` if they span upload
        // batches. The assembled position must be deterministic (the max) and
        // independent of arrival order, or clients diverge.
        let record = WireEvent::baseline(
            pos(100, "cover"),
            Body::single(1, baseline_payload(&[("A", json!({ "x": 1 }))])),
        );
        let json = serde_json::to_string(&record).unwrap();
        let chunks = split_frames(&json, 15);
        let count = chunks.len() as u32;
        assert!(count > 1, "need multiple frames, got {count}");

        // Each frame gets a distinct `modified`; the newest is 100 + count * 10.
        let make = || -> Vec<StagedRecord> {
            chunks
                .iter()
                .cloned()
                .enumerate()
                .map(|(i, data)| StagedRecord {
                    group: Guid::new("g"),
                    modified: ServerTimestamp::from_millis(100 + (i as i64 + 1) * 10),
                    raw: RawRecord::Frame {
                        part: i as u32,
                        part_count: count,
                        data,
                    },
                })
                .collect()
        };
        let expected = EventPosition::new(
            ServerTimestamp::from_millis(100 + count as i64 * 10),
            Guid::new("g"),
        );

        assert_eq!(assemble_events(make(), 1)[0].position, expected);
        let mut reversed = make();
        reversed.reverse();
        assert_eq!(assemble_events(reversed, 1)[0].position, expected);
    }

    #[test]
    fn assemble_events_rejects_incomplete_group() {
        // Two frames, but they claim a group of three.
        let record = WireEvent::baseline(
            pos(100, "cover"),
            Body::single(1, baseline_payload(&[("A", json!({ "x": 1 }))])),
        );
        let mut frames = frames_of(&record, "g", 15);
        frames.truncate(2);
        for f in &mut frames {
            if let RawRecord::Frame { part_count, .. } = &mut f.raw {
                *part_count = 3;
            }
        }
        assert!(assemble_events(frames, 1).is_empty());
    }

    #[test]
    fn new_client_cold_starts_from_a_split_baseline() {
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer.clone());

        // A baseline framed tiny (so it splits), delivered out of order.
        let mut frames = baseline_frames(
            "g",
            100,
            "e100",
            &[("A", json!({ "name": "a" })), ("B", json!({ "name": "b" }))],
            40,
        );
        assert!(frames.len() > 1, "baseline should have split into frames");
        frames.reverse();

        let mut telem = sync15::telemetry::Engine::new("containers");
        engine.stage_incoming(frames, &mut telem).unwrap();
        engine
            .apply(ServerTimestamp::from_millis(150), &mut telem)
            .unwrap();

        let baselines = reducer.baselines.lock().unwrap();
        assert_eq!(baselines.len(), 1);
        // Reassembled to the original payload despite reversed frame delivery.
        let names: Vec<String> = baselines[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["payload"]["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn incomplete_baseline_group_is_ignored() {
        let reducer = Arc::new(RecordingReducer::default());
        let engine = engine_with(reducer.clone());

        // Only frame 0 of a 2-frame group arrives.
        let frame = Wire::Frame(WirePart {
            group: Guid::new("g"),
            part: 0,
            part_count: 2,
            data: "{".to_string(),
        });
        let mut telem = sync15::telemetry::Engine::new("containers");
        engine
            .stage_incoming(vec![incoming(&frame, 150, "g_0")], &mut telem)
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
        *reducer.baseline.lock().unwrap() = baseline_payload(&[("X", json!({ "name": "x" }))]);
        let engine = engine_with(reducer.clone())
            .with_baseline_config(BaselineConfig {
                events_per_baseline: Some(1),
                baseline_timeout_ms: None,
            })
            .with_max_record_payload_bytes(usize::MAX);
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

    #[test]
    fn gate_advances_only_after_all_event_frames_confirmed() {
        let reducer = Arc::new(RecordingReducer::default());
        // Tiny record budget forces a local event to split into byte frames.
        let engine = engine_with(reducer)
            .with_baseline_config(BaselineConfig {
                events_per_baseline: None,
                baseline_timeout_ms: None,
            })
            .with_max_record_payload_bytes(200);
        let mut telem = sync15::telemetry::Engine::new("containers");

        // A local event too big for one record: it frames into several parts.
        let entity = Guid::new("bigXbigXbigX");
        engine
            .record_local_event(
                "created",
                entity.clone(),
                json!({ "name": "x".repeat(1000) }),
            )
            .unwrap();
        let outgoing = engine
            .apply(ServerTimestamp::from_millis(100), &mut telem)
            .unwrap();
        assert!(
            outgoing.len() > 1,
            "the event should have framed into >1 part, got {}",
            outgoing.len()
        );

        let gate = GateKey::new("created", entity);
        let ids: Vec<Guid> = outgoing.iter().map(|b| b.envelope.id.clone()).collect();

        // Confirming only the first frame must not advance the gate: the server
        // holds a partial event no peer can reassemble.
        engine
            .set_uploaded(ServerTimestamp::from_millis(110), vec![ids[0].clone()])
            .unwrap();
        assert!(
            engine.meta.position(&gate).unwrap().is_none(),
            "gate must not advance until every frame is confirmed"
        );

        // Confirming the remaining frames completes the event; the gate advances.
        engine
            .set_uploaded(ServerTimestamp::from_millis(120), ids[1..].to_vec())
            .unwrap();
        assert!(
            engine.meta.position(&gate).unwrap().is_some(),
            "gate should advance once every frame is confirmed"
        );
    }
}
