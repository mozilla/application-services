# Split ads-client telemetry into success metrics, expected errors and unexpected errors

* Status: proposed
* Deciders: ads-client team
* Date: 2026-10-21

Technical Story: [Bug 2047640](https://bugzilla.mozilla.org/show_bug.cgi?id=2047640), [Bug 2012752](https://bugzilla.mozilla.org/show_bug.cgi?id=2012752)

Paths are relative to `components/ads-client/` unless noted.

## Context and Problem Statement

ads-client records five Glean metrics today: four `labeled_string`s and one `labeled_counter`, all in the `metrics` ping. They mix success signals, expected failures and bugs, and they don't answer the questions we have:

- **There is no success rate.**
  - `client_operation_total` is only recorded when an operation succeeds (`src/client.rs:119,163,179,204,230,257`), although its description says "attempted".
  - A `labeled_string` keeps one value per label per ping, so the derived table counts pings with an error, not errors.
- **Bug 2047640 is caused by our error strings.** It reports `invalid_overflow` on `client_error`: values longer than Glean's 255-byte string limit.
  - We store the nested `to_string()` of the error (`src/telemetry.rs:146-148`).
  - That string includes whole HTTP response bodies (`src/mars/error.rs:76-90`) and URLs, so it is a privacy problem as well as an overflow.
- **Every change is expensive.**
  - The derived table `ads_derived.ads_client_operations_and_errors_hourly_v1` hard-codes 39 label columns across 4 sources.
  - It misses Fenix beta and iOS beta/nightly, and it cannot be backfilled.
  - Desktop re-declares our metrics in `browser/components/newtab/metrics.yaml`, and the labels have already drifted: `trim_failed` and `store_*` land in `__other__` on mobile.
- **We want to record from Rust with glean-sym, which has hard limits.** In 70.3.0:
  - it cannot generate any `labeled_*` type;
  - `DualLabeledCounterMetric::get` segfaults, because of an FFI return-type mismatch;
  - `ObjectMetric` only works untyped, through `set_string`;
  - it panics if the host library is missing;
  - it does not run on Desktop at all (bug 2068959).
- **A request timeout cannot be detected today.** viaduct turns every transport failure into `BackendError(String)` (`components/viaduct/src/error.rs:12`).

How should we structure ads-client telemetry so that:
- a few metrics tell us how well the client works;
- expected failures are cheap aggregates;
- unexpected errors arrive as a detailed list we can act on quickly;
- adding an error kind does not require a new derived-table migration or a new data review?

## Decision Drivers

* Each success metric must answer a question and point at a lever we can pull.
* Error counts must be correct occurrences with a matching denominator.
* Unexpected errors must be listed one row per error, with enough detail to fix them.
* A new error kind must not change Glean metric definitions, BigQuery schemas or the derived SQL.
* Data stays in categories 1-2: no URLs, bodies, paths or identifiers in error data.
* Everything must work with glean-sym today on Android and iOS, and must have a Desktop path.

## Considered Options

Expected errors:
1. `labeled_counter` keyed by error kind.
2. `dual_labeled_counter` of operation × outcome.
3. One `event` with `kind` as an extra value.

Unexpected errors:
1. `text` metric in a custom ping.
2. `object` metric in a custom ping.
3. `event` with string extras.
4. error-support's `rust-component-errors` ping (`report_error!`).

## Decision Outcome

Chosen options:
- **Expected errors:** option 3, an event.
- **Unexpected errors:** option 2, an object in a custom ping.
- **Success metrics:** a small set of unlabeled `rate`s, `timing_distribution`s and one `counter`.
- **Enforcement:** one exhaustive classifier in Rust decides the tier of every error variant.

| Tier | Glean shape | Where it lands | Cost of a new kind |
|---|---|---|---|
| 1. Success | 5 `rate`s, 2 `timing_distribution`s, 1 `counter` | `metrics` ping | n/a (deliberately small) |
| 2. Expected errors | event `ads_client.expected_error`, with `kind` as an extra value | `events` ping → `events_stream`, one row per error | none: a value, not a column or label |
| 3. Unexpected errors | object `ads_client.error_reports` in a new `ads-client-errors` ping, one record per distinct error, submitted when a new error appears | one table per app, shown as a list | none: a value or a context key |

### Positive Consequences

* Real rates: request, impression, click, fill and cache-hit rates, plus latency that includes failures.
* Bug 2047640 goes away: no `labeled_string`, and no `Display` text reaches Glean.
* A new error kind needs no yaml, SQL or schema change. The derived table becomes long-format and can be backfilled.
* Unexpected errors become a deduplicated list with an error chain, a location, a step trace and typed context.
* Only types that glean-sym, glean-fog and FOG JS all support.

### Negative Consequences

* A new custom ping: one probe-scraper change, one data review and a consultation with the Glean team.
* Desktop needs its own path until glean-sym works there: glean-fog in libxul, or a temporary generic sink.
* Events and custom pings are not hourly aggregates. Fast detection moves to hourly scheduled queries on the `_live` tables.
* Detecting a timeout properly needs a change to viaduct.

## Pros and Cons of the Options

### Expected errors: `labeled_counter`

* Good, because it is a compact count.
* Bad, because glean-sym cannot generate labeled types.
* Bad, because a new kind is a new yaml label, plus a review, plus SQL that lists labels.

### Expected errors: `dual_labeled_counter`

* Good, because it gives per-operation denominators in one metric.
* Bad, because `get()` segfaults under glean-sym 70.3.0. A segfault cannot be caught.

### Expected errors: event

* Good, because a new kind is a value: it lands in `events_stream` as JSON, with one row per occurrence.
* Good, because it carries operation, stage, status/transport code and cache source.
* Bad, because of volume. It is mitigated by a per-process cap; exact totals live in the tier 1 denominators.

### Unexpected errors: `text` in a custom ping

* Good, because it holds up to 200 KB.
* Bad, because Glean only allows it in custom pings at data category 3/4 (`web_activity`/`highly_sensitive`), and asks teams to consult it first.

### Unexpected errors: `object` in a custom ping

* Good, because it has no sensitivity restriction, lands as a JSON column, and can hold an array of structured records.
* Good, because there are precedents at `technical`: `addons_manager.exception` and `crash.java_exception`.
* Bad, because glean-sym only offers untyped `set_string`, and Desktop validates the value against the yaml `structure`.

### Unexpected errors: event

* Good, because it is simple and listed automatically.
* Bad, because extra values are capped at 500 bytes, with at most 50 extras.

### Unexpected errors: `rust-component-errors`

* Good, because it already exists, is reviewed, and has a dashboard generator.
* Bad, because `details` is capped at 255 B, the rate limiter is process-global and keyed on "ads", the ping is shared with another team, and it has no iOS or Desktop table.

## Detailed Design

### Related bugs

These were found while preparing this ADR. They distort the metrics, so they get fixed (or filed) in Phase 0:

* **The HTTP cache stores non-200 responses.**
  * `should_cache` only checks `no-store` (`src/http_cache/cache_control.rs:54-56`).
  * Responses are stored (`src/http_cache/strategy.rs:51-70`) before the status check (`src/mars/transport.rs:65`).
  * So a 5xx or captive-portal reply is replayed for the whole TTL.
* **Desktop's request timeout is in the wrong unit.** viaduct-necko treats the millisecond timeout as seconds (firefox `services/application-services/components/viaduct-necko/backend.cpp:201`, `aTimeoutSecs * 1000`, against `ClientSettings.timeout` = 60 000 ms), so it fires after about 16.7 h.
* **The context id is never persisted.**
  * `DefaultContextIdCallback` does nothing, so every client build gets a new id, and the 3-day rotation only happens in long-lived processes.
  * The rotation DELETE runs synchronously inside `request_ads`.

### 1. Principles

**The three tiers**
- **Tier 1 (success):** a ratio or a distribution over *every attempt*. Each has a "better" direction and a lever we can pull. It never carries error detail.
- **Tier 2 (expected):** failures that a correct ads-client, a correct MARS and a correct embedder still produce in the field: network, server load, device state, shutdown. We act on their rate. Recorded as a closed `kind` with no free text.
- **Tier 3 (unexpected):** a bug or a broken contract, whether ours, MARS's or the embedder's. We act on each distinct one. Each becomes a scrubbed, deduplicated record in a list.

**The rule that decides the tier:** "Could this happen on a healthy device, with a working network, and every party behaving correctly?" Yes means T2; no means T3.

**How the code enforces it (`src/telemetry/classify.rs`)**
- `trait Classify { fn classify(&self) -> Classified; fn message(&self) -> Cow<'static, str>; }` is implemented for every error enum.
- Exhaustive matches: rustc enforces them for our enums, for viaduct and for `open_database`. `clippy::wildcard_enum_match_arm` bans `_` on local enums.
- There are two `// By design:` fallbacks, for the `#[non_exhaustive]` types `rusqlite::Error` and `rusqlite::ffi::ErrorCode`.
- Recording APIs accept only `&impl Classify`, so no raw `Display` string ever reaches Glean.

**Accounting check**
- Every T2 event and T3 record carries `fatal: bool`, meaning the public operation returned `Err`.
- For request and callback operations, `fatal T2 + fatal T3 ≈ denominator − numerator`.
- This is a trend check, not an equality: the three pings ship on different schedules, T2 is capped and T3 is deduplicated.

### 2. Tier 1: success metrics (`metrics` ping)

| Metric | Type | Definition | Question → lever |
|---|---|---|---|
| `request_success_rate` | rate | Numerator: `request_{image,spoc,tile}_ads` returned Ok. Denominator: every call. | Do ad requests deliver? → the gap splits by T2 kind and T3 |
| `impression_success_rate` | rate | Numerator: HTTP 200. Denominator: every `record_impression`, including URL-parse failures (`lib.rs:79`) | Are billable impressions landing? → retry or queue (callbacks are never retried today) |
| `click_success_rate` | rate | Same, for `record_click` | Separate rate, because impressions drown clicks in a combined one |
| `fill_rate` | rate | On Ok responses. Numerator: ads returned to the caller. Denominator: ads requested (Σ `count`, which is 1 per image/tile placement, `ffi.rs:453,463`) | Are slots filled? → MARS inventory, or items we dropped (T3 `invalid_ad_item`) |
| `request_duration_direct`, `request_duration_ohttp` | timing_distribution (ms) | Every request that hit the network, Ok **or Err**. Cache hits are excluded. | How long do users wait? → preflight, the synchronous context-id DELETE, the OHTTP config fetch. Failures are included so p95 can't improve by turning slow requests into errors. |
| `cache_hit_rate` | rate | Numerator: CacheFirst main requests answered from cache with a 200 that parses. Denominator: all CacheFirst main requests, including those made when the cache failed to build. The preflight is excluded. | Is the cache saving round trips? → TTL tuning |
| `context_id_rotations` | counter | +1 each time a new context id is issued (client build or 3-day rotation), via a new `AdsContextIdCallback::persist` | How often MARS sees a new id per client-day. The design intent is about 0.33 a day; expect about launches per day today. → the persistence decision |

Notes:
- **report_ad has no T1 rate.** It is rare and user-initiated; its failures still go to T2 and T3.
- **Latency is split by OHTTP.** OHTTP is a per-call option driven by a Desktop pref (`AdsClient.sys.mjs:37-38`). A single distribution would move with a rollout rather than with a regression.
- **Units.** Timing distributions are stored in **nanoseconds** whatever `time_unit` says. Dashboards must label them accordingly.

**Where it is recorded**
- An `OpScope` is begun in each exported method (`lib.rs:57-166`).
- `#[handle_error]` returns early on `?`, e.g. the URL parse at `lib.rs:64/79/96`. So each body moves into an inner closure: bind the result, call `scope.finish(&result, info)`, then return.
- `transport.send` returns its cache outcomes instead of recording them (`transport.rs:62-64`). The **error** path must carry them too: today the `?` at `strategy.rs:36/51` drops a `LookupFailed` when the network then fails.

### 3. Tier 2: expected errors

`ads_client.expected_error` is an `event` in the default `events` ping, so each occurrence becomes one row in `events_stream` with its extras as JSON.

**Extras:** `operation`, `kind`, `stage`, `code`, `source` (network|cache), `ohttp` (bool), `fatal` (bool).
- `in_session: false`. rust_sym ignores the setting and always uses false; keep the yaml consistent so FOG behaves the same.

**Cap:** 50 events per (operation, kind) per process. Exact totals for fatal request and callback failures survive in the T1 denominators.

**Kinds**, a closed set of values:
- `timeout`, `offline`, `dns`, `connection`, `tls`, `proxy`, `cancelled`, `network_other`
- `ohttp_config`, `ohttp_relay`
- `http_server` (5xx), `http_throttled` (429), `callback_gone` (404/410 on callbacks)
- `captive_response` (a 200 with a non-JSON content type)
- `shutdown`, `storage_busy`, `storage_downgrade`

**`stage`** is derived from the last `step!` in the operation's trace, not from the error variant. Preflight failures arrive as a plain `FetchAdsError::Request` or `CallbackRequestError` (`mars.rs:85, 231-236`).

**Data categories:** technical + interaction. The event records timestamped click and impression failures, keyed by client_id.

**Making "timeout" detectable**
- **(a) Now, in ads-client.** `transport_kind(&str)` reads the code out of the `BackendError` text:
  - necko: the `(0x%08x)` suffix, e.g. `0x804B000E` timeout, `0x804B0010` offline, `0x804B001E` dns;
  - GeckoView: `error=0x..` or `SocketTimeoutException`;
  - iOS: reqwest drops the cause, so these stay `network_other`.
  - The raw code goes into `code`. Unit-test it against recorded real strings; viaduct-dev (minreq) in tests can't produce them.
- **(b) Properly, in viaduct.** Add `TransportError { kind, code }`, classified by each backend:
  - necko, in mozilla-central;
  - viaduct-backend-rust, with `is_timeout`/`is_connect` plus `without_url()`;
  - a try/catch in `FetchBackend.kt`.
  - Then delete (a). Adding the variant breaks consumers' exhaustive `when`/`switch` statements.
- **(c) Desktop:** `timeout` only means our 60 s once the viaduct-necko unit bug is fixed.

#### Error variant → tier and kind

Wrappers (`ComponentError`, `RequestAdsError::FetchAds`, `Record*Error::CallbackRequest`, `FetchAdsError::{BuildRequest,HTTPError,Request,Sqlite,Preflight}`, `CallbackRequestError::{HTTPError,Request}`, the builder `Database` variants, `CacheOutcome::*Failed`) are classified by their inner error.

| Variant | Tier: kind |
|---|---|
| `BuildRequestError::{DuplicatePlacementId, EmptyConfig}` (mars/error.rs:11,14) | T3 `invalid_request_config` |
| `HTTPError::BadRequest` (400) | T3 `http_bad_request` |
| `HTTPError::Server` (5xx) | T2 `http_server` |
| `HTTPError::Unexpected` 408 / 429 | T2 `timeout` / `http_throttled` |
| `HTTPError::Unexpected` 404/410 on a callback | T2 `callback_gone` |
| `HTTPError::Unexpected`, any other status (a 2xx other than 200, 3xx, 401, 403, other 4xx, 404 on ads or preflight) | T3 `http_unexpected_status` |
| `CallbackRequestError::InvalidUrl` (lib.rs:64,79,96) | T3 `invalid_callback_url` |
| `CallbackRequestError::Json` (preflight body) / `FetchAdsError::Json` (mars.rs:89) | Non-JSON content type: T2 `captive_response`. Otherwise T3 `invalid_preflight` / `invalid_response` |
| A placement value that is not an array (ad_response.rs:24, silently skipped today) | T3 `invalid_placement` |
| Invalid item `serde_json::Error` (ad_response.rs:27-31) | T3 `invalid_ad_item`, one record per response |
| Stateful store Json (ads_store/store.rs:110,125) | T3 `storage_json` |
| `RequestAdsError::ContextId` | T3 `context_id` (unreachable today) |
| `HttpCacheBuilderError::{EmptyDbPath,InvalidMaxSize,InvalidTtl}`, `AdsStoreBuilderError::{EmptyDbPath,InvalidMaxSize}` | T3 `cache_config` |
| `open_database::Error::Shutdown` / `IncompatibleVersion` | T2 `shutdown` / `storage_downgrade` |
| `SqlError` with SQLITE_BUSY, FULL or IOERR | T2 `storage_busy` |
| Any other `SqlError`, and the rusqlite fallback | T3 `sql_error`, with `stage` = open, cache or store |
| `Corrupt` / `RecoveryError` | T3 `db_corrupt` / `db_recovery` |
| `ViaductError::BackendError` | T2, kind taken from the code (above), otherwise `network_other` |
| `OhttpConfigFetchFailed` | T2 `ohttp_config`. Transport failures during the config fetch need the viaduct stage, (b) |
| `OhttpRequestError` | T2 `ohttp_relay`, with no text. After a viaduct split, encapsulation failures become T3 `ohttp_protocol` |
| `OhttpResponseError` | T3 `ohttp_protocol` |
| `UrlError` | T3 `invalid_url` |
| `NetworkError` (only an empty gateway host, ohttp.rs:65), `RequestHeaderError`, `BackendNotInitialized`, `NonTlsUrl`, `OhttpChannelNotConfigured`, `OhttpNotSupported` | T3 `misconfigured` |
| `FetchAdsError::UrlParse`, `TransportError::Json` | No producer: delete them |

### 4. Tier 3: unexpected errors

**Mechanism**
- A custom ping, `ads-client-errors`: `include_client_id: true`, `send_if_empty: false`, reasons `error|shutdown`.
- It carries one object metric, `ads_client.error_reports`: `lifetime: ping`, `technical`, recorded with `set_string(json)`.
- Rejected alternatives:
  - `text`: requires category 3/4.
  - error-support's `rust-component-errors` ping:
    - `details` is capped at 255 B;
    - its rate limiter is process-global and keyed on "ads";
    - the ping is shared with the sync team;
    - it has no iOS or Desktop table.

**Payload**
```json
{"v":1,"dropped":0,"records":[{
  "fp":"9f2c41d07a3be115","kind":"invalid_ad_item","op":"request_spoc_ads","stage":"parse","fatal":false,
  "chain":["RequestAdsError::FetchAds","FetchAdsError::Json","serde_json::Category::Data"],
  "message":"invalid type: <value>, expected a string",
  "location":"ads-client/src/mars/ad_response.rs:30",
  "steps":["context_id@client.rs:286","preflight@mars.rs:85","cache@strategy.rs:24","network@strategy.rs:51","status@transport.rs:65","parse@ad_response.rs:30"],
  "context":[{"k":"serde_path","v":"callbacks.impression"},{"k":"invalid_items","v":"2"},{"k":"status","v":"200"},
             {"k":"ohttp","v":"true"},{"k":"elapsed_ms","v":"412"},{"k":"host","v":"ads.mozilla.org"}],
  "count":1,"first_seen_s":37}]}
```

**Why `context` is a key/value list.** On Desktop, both glean-fog and FOG validate objects against the yaml `structure` with `deny_unknown_fields`. With typed fields, every new context field would need a yaml change and a re-sync on Desktop. With key/value pairs, it doesn't.
- The keys are allowlisted in `docs/telemetry.md`.
- The core fields stay typed, and a glean-fog CI test round-trips the golden JSON.

**What "the whole error and the trace" means here.** Libraries on the device are stripped of symbols:
- Android: AGP stripping and install-strip;
- iOS: built with `debuginfo=0`;
- libxul: stripped at packaging, and built panic=abort.

A Rust backtrace would therefore be `<unknown>` frames, and capturing one takes a process-global lock. Each record carries these instead:
1. **`chain`:** the `source()` chain as variant names, at most 8 levels.
2. **`message`:** from `Classify::message()`, a static template per variant plus typed fields.
   - serde errors get their scrubbed `Display`.
   - `HTTPError` (it embeds the body) and `DuplicatePlacementId` never use `Display`.
3. **`location`:** a `#[track_caller]` file:line.
4. **`steps`:** a logical trace. `step!(..)` appends `name@file:line` to a thread-local scope for the operation, at most 16 entries.
   - This is sound because every operation runs synchronously on its own thread (pollster).
   - Unlike error-support's breadcrumbs, nothing from other components gets mixed in.
5. **`context`:**
   - `serde_path`, from re-running `serde_path_to_error` only on `Err`. That crate is 0.1.11, already in `Cargo.lock`, and vendored and audited in mozilla-central; sync15 already does exactly this.
   - Also: item index, invalid_items, status, body_bytes, content_type, host, elapsed_ms, the SQLite extended code, the statement name, the transport code.

**Fingerprint**
- `fp = FNV-1a-64(kind|op|stage|leaf variant|serde_path|sqlite_code|http_status)`.
- No wrapper variants, so it survives refactors. No line numbers and no free text.
- It is the error's identity in the list and in alerts.

**Scrubber (`telemetry/scrub.rs`), as defence in depth**
- Only allowlisted fields are written.
- HTTP bodies are never recorded; keep `body_bytes` and the content type instead.
- URLs: only the host is kept, and any `scheme://…` inside a message becomes `<url>`.
- Values:
  - Debug-escaped quoted strings become `<value>`, using a real parser rather than a naive regex;
  - backticked tokens that are not identifiers (numbers, bools, chars) also become `<value>`.
- `<uuid>`; POSIX, Windows (drive letter, UNC, `\\?\`) and `file://` paths become `<path>`; runs of 12 or more hex/digit characters become `<n>`. Strip `[no-sentry]`.
- Never record the context id, user agent, geo headers, request bodies or placement ids.
- Caps: 512 B per string, 4 KB per record, 10 records per ping.
- A hostile-input table test asserts that none of these survive into the JSON.

**Deduplication and submission.** A process-wide static `Reporter`, using the crate's `Clock`.
- **New vs repeat:**
  - The first occurrence of a fingerprint gets a full record. `set_string` and `submit("error")` run **immediately**, and pending repeat counts ride along.
  - Repeats only increment `count`. They never trigger a ping on their own; they go out with the next new-fingerprint ping or at shutdown.
  - `set_string` runs after every append, so pending data sits in Glean's ping storage.
- **Caps:**
  - 10 distinct fingerprints per process; `dropped` counts the rest.
  - At most 5 pings per client per day, persisted in the HTTP cache database, so a deterministic config error doesn't send a ping on every launch forever.
  - State both caps in `pings.yaml` and in the data review.
- **Invalid ad items** are aggregated into one record per response.
- **Never touch Glean from `build()`.** Build-time errors (client.rs:70,83) are buffered.
- **Android:** before the first glean-sym call, probe for `dlopen("libxul.so", RTLD_NOLOAD)`. If libxul isn't loaded yet, buffer everything, rates included, and probe again on the next call.
  - A cache hit can serve a request before GeckoView is up; Fenix uses a 30-minute TTL.
  - Ask Glean to make this glean-sym's own behaviour.

**The list (view over the per-app custom-ping tables)**
```sql
SELECT app, submission_timestamp, normalized_channel, client_info.app_display_version AS version,
  client_info.client_id, JSON_VALUE(r,'$.fp') AS fp, JSON_VALUE(r,'$.kind') AS kind, JSON_VALUE(r,'$.op') AS op,
  JSON_VALUE(r,'$.stage') AS stage, JSON_VALUE(r,'$.message') AS message,
  SAFE_CAST(JSON_VALUE(r,'$.count') AS INT64) AS occurrences, r AS record
FROM pings p, UNNEST(JSON_QUERY_ARRAY(p.metrics.object.ads_client_error_reports, '$.records')) r
```
The dashboard groups by `fp` and shows first and last seen, Σ occurrences, distinct clients, apps and versions, and the latest full record.

### 5. Recording architecture

**Module layout**
- `telemetry.rs` keeps its free functions and its "infallible and silent" contract.
  - New: `begin(op) -> OpScope`, `step!`, `OpScope::finish`, `record_side_error`, `record_context_id_rotation`.
- New modules: `telemetry/{classify,scrub,report}.rs`.
- `telemetry/backend.rs` gets a `Recorder` trait: rate, timing, counter, expected_error, error_reports. Implementations:
  - `GleanRecorder`, compiled with `glean-sym` or `glean-fog`. It is one file, because both generated APIs share the calls it uses.
  - `SinkRecorder`, `NoOp`, and `Capture` (`cfg(test)`).

**Panic guard.** glean-sym calls run inside `catch_unwind`. The first panic disables Glean recording for the rest of the process; a-s builds with panic=unwind. A segfault cannot be caught, which is another reason not to use `dual_labeled_counter`.

**YAML**
- The new definitions go in `components/ads-client/glean/{metrics,pings}.yaml`, read only by `build.rs`. Add them to probe-scraper's ads-client `metrics_files`/`ping_files`.
- The v1 `metrics.yaml` stays for Kotlin/Swift codegen until cleanup. rust_sym **cannot** compile the v1 labeled metrics (15 errors, verified). The split also keeps the object out of Kotlin, which would need kotlinx-serialization, and out of Swift, where e.g. a field named `repeat` is a keyword.

**Build and features**
- `build.rs` is copied from places (`components/places/build.rs:10-21`):
  - code is generated only when one of the features is on;
  - it rewrites `ObjectMetric<…>` to `ObjectMetric` for rust_sym;
  - `glean-fog` wins when both features are on, which happens under `--all-features` in CI.
- Features: `glean-sym = ["dep:glean-sym"]` and `glean-fog = ["dep:glean"]`.
- `megazords/full` and `megazords/ios-rust` enable `ads-client/glean-sym`. **That line is the per-component kill switch** (precedent: #7386). Fenix's in-tree `fenix-dylib` needs the same line.

**Version lockstep.** glean-sym only checks the UniFFI contract version (30), so any other drift is silent undefined behaviour.
- Check it where the binary is assembled:
  - mozilla-central: glean-sym major.minor == glean-core;
  - the firefox-ios dependency bump: glean-swift vs the a-s glean-sym.
- In a-s, only check glean-sym against glean-build.

**Tests**, all against `Capture`:
- accounting scenarios, using a seam that injects a `ViaductError`;
- golden T3 JSON;
- the scrub table;
- dedup and caps with `TestClock`;
- a classifier snapshot;
- a drift test that every emitted value is documented in `docs/telemetry.md`;
- a glean-fog CI test that round-trips the golden JSON through the generated object type.

**Desktop**
- **Target: glean-fog in libxul.**
  - Enable `ads-client/glean-fog` in gkrust-uniffi-components.
  - List `third_party/application-services/components/ads-client/glean/*.yaml` in `metrics_index.py` and in probe-scraper's `firefox_desktop` entry.
  - Delete the newtab `ads_client` copy in the same patch.
  - Recording is parent-process only, which fits: ads-client is driven from `AdsClient.sys.mjs`.
  - Nobody uses glean-fog in mozilla-central yet, so start with a try push.
- **Interim, if that slips.** An additive `MozAdsTelemetrySink`: one generic callback with 5 methods, set through `MozAdsClientBuilder::telemetry_sink()`.
  - It needs `MozAdsTelemetrySink = "FireAndForget"` in mozilla-central's `uniffi-bindgen-gecko-js/config.toml` in the vendoring patch.
  - Mirror the definitions into newtab's yaml once.
  - The JS feature-detects the sink, because newtab trainhops across 3 Firefox versions.
  - Time-box it to one release.

### 6. Data platform

**Tiers 1 and 2: `ads_derived.ads_client_health_daily_v2`**
- Long format: `(submission_date, app, channel, os, app_version, metric, operation, kind, numerator, denominator, clients)`.
- It reads the app-level `metrics` and `events_stream` views, which brings in the betas.
- It is incremental per day, so it can be backfilled.
- A Jinja availability map per (app, metric) controls which columns are referenced. A column missing from one app's view is a compile error that no `WHERE` clause can avoid.
- A new kind or operation needs **no SQL change**.

**Distributions:** one line each in bigquery-etl `rust_component_metrics` (Distribution template). Units are nanoseconds, and the template's owner is hard-coded to the sync team, so agree that with them or add an owners field.

**Tier 3:** views, not derived tables.
- `ads.ads_client_errors`: a union of the stable tables.
- `ads.ads_client_errors_live`: 48 h, deduplicated on `document_id`.

**Alerts**
- Hourly scheduled queries on the `_live` tables, posting to Slack:
  - new fingerprints, seen on at least 3 clients and absent for the previous 28 days;
  - each T2 kind's share against a 7-day baseline.
- Daily Bigeye bounds on the v2 rates and the duration p95.
- T1 cannot give same-day detection, because the metrics ping is daily. v1's "hourly" only meant the hour of submission.

**Dashboards**
- A lookml-generator namespace with 3 generic measures: Σ numerator, Σ denominator, ratio.
- The error list goes in `tools/generate-rust-dashboards`: add `Component::AdsClient` and an error-list module. lookml-generator skips object columns.

**Retiring v1:** after the parallel run, and keep its data.

### 7. Data review

**One review for the whole v2 set**
- Mobile: the form on the a-s PR.
- Desktop: Phabricator classification when the yaml enters mozilla-central.
- Consult the Glean team on an object array in a component-owned ping.

**Categories**
- The rates and `expected_error`: technical + interaction.
- Everything else: technical.
- T3: category 1 ("crashes and errors"), with a client_id.
- Nothing at category 3 or 4.

**Rules, not values.**
- Operations, kinds, stages, chain names and context keys are documented in `components/ads-client/docs/telemetry.md`, linked from the yaml descriptions.
- So a new kind or context key is a code+doc PR that doesn't touch the yaml, and on Desktop it doesn't trigger `needs-data-classification`.
- Whether the steward accepts this is **the** question for the review (not confirmed yet), especially for context keys.

**Still needs review:** a new metric, a new event extra, a new typed record field, a change to sensitivity or client_id, or larger caps.

### 8. Migration (one concern per PR)

**Phase 0, now**
- 0.1 **Hotfix bug 2047640** in `telemetry.rs`: replace bodies with `<N bytes>`, strip URL query and fragment, truncate to 255 B on a char boundary (as #7300 did). Reply on the bug.
- 0.2 Add `trim_failed` to `metrics.yaml`.
- 0.3 **Stop caching non-200 responses**, before any v2 baselines are set.
- 0.4 File bugs:
  - Glean: the `DualLabeledCounterMetric::get` segfault, and rust_sym's `ObjectMetric<T>`.
  - Firefox: the viaduct-necko timeout unit.

**Phase 1, no new data**
- 1.1 Delete the dead variants.
- 1.2 Add `classify`, `scrub`, `report`, `OpScope`, `step!` and `Capture`, with tests.
- 1.3 Carry cache outcomes on the error path.
- 1.4 viaduct `TransportError` kinds (its own track: a-s and mozilla-central).

**Phase 2, mobile collection**
- 2.1 Data review and Glean consultation. This gates the rest of the phase.
- 2.2 probe-scraper entries.
- 2.3 Features, build.rs, `Recorder`, panic guard, libxul probe, CI; then the T1 metrics.
- 2.4 The T2 event.
- 2.5 The T3 ping and reporter.
- 2.6 Context-id callback and counter.
- 2.7 The megazords enable the feature.

**Phase 3, Desktop:** a glean-fog try push, then land it; otherwise the interim sink, within one release.

**Phase 4, data platform:** the v2 table, views, alerts and dashboards.

**Phase 5, parallel run** of at least one release:
- Compare distinct clients with at least one error per operation per activity day.
- Not ping counts: the three pings ship on different schedules.

**Phase 6, cleanup.** Remove:
- the v1 metrics, the legacy `record_*` functions and `MozAdsTelemetry`;
- `AdsClientTelemetry.kt` and `AdsClientTelemetry.swift`, `gleanYamlFiles` and the iOS codegen;
- the newtab copy;
- the v1 DAG, LookML and Bigeye.

### 9. Risks, open questions, asks

**Risks**
- glean-sym is experimental: bug 2043773 crashed, and #7386 disabled it.
- Version drift is silent undefined behaviour.
- Android load order.
- A release-wide MARS break: bounded by the daily cap.
- The scrubber missing something: bounded by the allowlist and the tests.
- The Desktop timeline.

**Open questions for the team**
- `include_client_id` on the errors ping?
- Persist the context id?
- The daily ping cap value.
- Is a daily T1 grain acceptable?
- Which megazord does Fenix ship, a-s `full` or mozilla-central `fenix-dylib`?

**Asks for the Glean team**
1. Fix the `get()` segfault.
2. Fix rust_sym's `ObjectMetric<T>`, or bless `set_string`.
3. Add per-function checksum checks to glean-sym.
4. Endorse an object array in a component-owned ping.
5. Can server knobs disable glean-sym metrics and pings, as a remote kill switch?
6. Does unsent ping-lifetime data survive a restart?
7. Endorse glean-fog in libxul.
8. Give glean-sym a "host not loaded yet" mode instead of a panic, plus an in-process test host.

## Links

* [Bug 2047640](https://bugzilla.mozilla.org/show_bug.cgi?id=2047640): `invalid_overflow` on `ads_client.client_error`
* [Bug 2012752](https://bugzilla.mozilla.org/show_bug.cgi?id=2012752): glean-sym meta bug
* [Bug 2068959](https://bugzilla.mozilla.org/show_bug.cgi?id=2068959): glean-sym on Desktop
* [Glean object metric](https://mozilla.github.io/glean/book/reference/metrics/object.html), [text metric](https://mozilla.github.io/glean/book/reference/metrics/text.html), [event metric](https://mozilla.github.io/glean/book/reference/metrics/event.html)
* [Current derived table](https://github.com/mozilla/bigquery-etl/tree/main/sql/moz-fx-data-shared-prod/ads_derived/ads_client_operations_and_errors_hourly_v1)

<!-- markdownlint-disable-file MD013 -->
