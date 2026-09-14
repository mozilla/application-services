# event_based_sync_engine

A generic, storage-agnostic, event-based sync engine. It transports opaque,
GUID-keyed *events* over `sync15` and folds them into the caller's storage through
a reducer, instead of dumping and reconciling a whole data structure.

The engine never interprets a payload: it orders, gates, resolves schema, and
dispatches. All domain logic lives in the caller's `EventReducer`.

An *event* is the logical unit the engine orders and applies: an `IncomingEvent`,
resolved to a `ResolvedEvent` for the reducer.
A *record* is its form on the wire: a `WireEvent` serialized into one BSO, or, when
it is bigger than `max_record_payload_bytes`, several `WirePart` frames sharing a
group id (each parsed into a `StagedRecord` on the way in).
So one `WireEvent` maps to on or more records, which `assemble_events` reassembles
back into an `IncomingEvent` before applying.

## Event lifecycle

### Incoming: server to reducer

```mermaid
flowchart TD
    S(["Server BSOs"]) --> ST["stage_incoming<br/>park BSOs"]
    ST --> AP["apply: drain staged"]
    AP --> SR["staged_records<br/>BSO to StagedRecord (Whole or Frame)"]
    SR --> AE["assemble_events<br/>regroup frames into IncomingEvent"]
    AE -->|"group incomplete"| SK(["skip until every frame arrives"])
    AE --> SPLIT{"reserved event type?"}
    SPLIT -->|"baseline"| BL["reducer.apply_baseline (cold start)<br/>advance floor, prune gate below it"]
    SPLIT -->|"domain event"| ORD["order by EventPosition, ascending"]
    ORD --> GATE{"supersedes<br/>floor + last-seen?"}
    GATE -->|"no"| IG(["ignore"])
    GATE -->|"yes"| MIG["SchemaRegistry migrate-on-read<br/>into ResolvedEvent"]
    MIG --> AEV["reducer.apply_event"]
    AEV --> GP["MetadataStore: set gate position"]
    BL --> MP["MetadataStore: floor + gate"]
```

1. `stage_incoming` parks the downloaded `IncomingBso`s.
2. `apply` drains them:
   - `staged_records` parses each `IncomingBso` into a `StagedRecord` (a
     `RawRecord::Whole`, or one `RawRecord::Frame` of a split one).
   - `assemble_events` reassembles frames by group into whole `IncomingEvent`s. An
     incomplete group is skipped until every frame is present.
   - each `IncomingEvent` is ordered by its `EventPosition` and gated per `GateKey`:
     it applies only if it `supersedes` the last-seen `EventPosition` for its key
     (and the floor).
   - `SchemaRegistry` migrates a winner on read to the current schema, then it is
     dispatched to the `EventReducer` as a `ResolvedEvent`.
   - a baseline `WireEvent` instead advances the *floor* in the `MetadataStore` and
     prunes the gate below it.

### Outgoing: local edit to server

```mermaid
flowchart TD
    LE(["Local edit"]) --> RLE["record_local_event<br/>dual-write into EventBody, build WireEvent"]
    RLE --> FR{"fits max_record_payload_bytes?"}
    FR -->|"yes"| ONE["one BSO"]
    FR -->|"no"| MANY["split into group_0.. WirePart frames (base64)"]
    ONE --> Q["queue PendingOutgoing"]
    MANY --> Q
    Q --> AP["apply: emit BSOs<br/>remember AwaitingConfirmation {remaining, OnComplete}"]
    AP --> UP(["upload to server"])
    UP --> SU["set_uploaded(ids, modified)<br/>drop confirmed frames"]
    SU --> DONE{"all frames of a group confirmed?"}
    DONE -->|"no"| WAIT(["wait for a later batch"])
    DONE -->|"yes"| OC{"OnComplete kind"}
    OC -->|"Event"| GATE["advance gate (set_position)<br/>count toward cadence"]
    OC -->|"Baseline"| FLOOR["advance floor, prune<br/>reset cadence"]
```

1. `record_local_event` serializes the change into a `WireEvent`, frames it into
   `OutgoingBso`s, and queues it as a `PendingOutgoing`.
2. `apply` emits the queued `OutgoingBso`s and remembers each as an
   `AwaitingConfirmation` (keyed by group id), tagged with the `OnComplete`
   side-effect to run once its upload is confirmed.
3. `set_uploaded` confirms each frame. Once every frame of a record lands, its
   `OnComplete` runs: `OnComplete::Event` advances the event's `GateKey` position,
   `OnComplete::Baseline` advances the floor. The gate and floor never move on a
   half-uploaded record.

The server assigns the authoritative `modified` timestamp, so a position is final
only after `set_uploaded`.