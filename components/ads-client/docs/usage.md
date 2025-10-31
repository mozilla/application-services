# Mozilla Ads Client (MAC) — UniFFI API Reference

## Overview

This document lists the Rust types and functions exposed via UniFFI by the `ads_client` component.
It only includes items that are part of the UniFFI surface. This document is aimed at users of the ads-client who want to know what is available to them.

---

## `MozAdsClient`

Top-level client object for requesting ads and recording lifecycle events.

```rust
pub struct MozAdsClient {
  ... // No public fields
}
```

#### Constructors

```rust
impl MozAdsClient {
  pub fn new(client_config: Option<MozAdsClientConfig>) -> Self
}
```

Creates a new ads client with an optional configuration object.
If a cache configuration is provided, the client will initialize an on-disk HTTP cache at the given path.

#### Methods

| Method                                                                                                                      | Return Type                                       | Description                                                                                                                                   |
| --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `clear_cache(&self)`                                                                                                        | `AdsClientApiResult<()>`                          | Clears the client’s HTTP cache. Returns an error if clearing fails.                                                                           |
| `cycle_context_id(&self)`                                                                                                   | `AdsClientApiResult<String>`                      | Rotates the client’s context ID and returns the **previous** ID.                                                                              |
| `record_click(&self, placement: MozAd)`                                                                                     | `AdsClientApiResult<()>`                          | Records a click for the given placement (fires the ad’s click callback).                                                                      |
| `record_impression(&self, placement: MozAd)`                                                                                | `AdsClientApiResult<()>`                          | Records an impression for the given placement (fires the ad’s impression callback).                                                           |
| `report_ad(&self, placement: MozAd)`                                                                                        | `AdsClientApiResult<()>`                          | Reports the given placement (fires the ad’s report callback).                                                                                 |
| `request_ads(&self, moz_ad_requests: Vec<MozAdsPlacementRequest>, options: Option<MozAdsRequestOptions>)`                   | `AdsClientApiResult<HashMap<String, MozAd>>`      | Requests one ad per placement. Optional `MozAdsRequestOptions` can adjust caching behavior. Returns a map keyed by `placement_id`.            |
| `request_multiple_ads(&self, moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>, options: Option<MozAdsRequestOptions>)` | `AdsClientApiResult<HashMap<String, Vec<MozAd>>>` | Requests up to `count` ads per placement. Optional `MozAdsRequestOptions` can adjust caching behavior. Returns a map keyed by `placement_id`. |

> **Notes**
>
> - We recommend that this client be initialized as a singleton or something similar so that multiple instances of the client do not exist at once.
> - Responses omit placements with no fill. Empty placements do not appear in the returned maps for either `request_ads` or `request_multiple_ads`.
> - The HTTP cache is internally managed. Configuration can be set with `MozAdsClientConfig`. Per-request cache settings can be set with `MozAdsRequestOptions`.
> - If `cache_config` is `None`, caching is disabled entirely.

---

## `MozAdsClientConfig`

Configuration for initializing the ads client.

```rust
pub struct MozAdsClientConfig {
  pub environment: Environment,
  pub cache_config: Option<MozAdsCacheConfig>,
}
```

| Field          | Type                        | Description                                                                                            |
| -------------- | --------------------------- | ------------------------------------------------------------------------------------------------------ |
| `environment`  | `Environment`               | Selects which MARS environment to connect to. Unless in a dev build, this value can only ever be Prod. |
| `cache_config` | `Option<MozAdsCacheConfig>` | Optional configuration for the internal cache.                                                         |

---

## `MozAdsCacheConfig`

Describes the behavior and location of the on-disk HTTP cache.

```rust
pub struct MozAdsCacheConfig {
  pub db_path: String,
  pub default_cache_ttl_seconds: Option<u64>,
  pub max_size_mib: Option<u64>,
}
```

| Field                       | Type          | Description                                                                          |
| --------------------------- | ------------- | ------------------------------------------------------------------------------------ |
| `db_path`                   | `String`      | Path to the SQLite database file used for cache storage. Required to enable caching. |
| `default_cache_ttl_seconds` | `Option<u64>` | Default TTL for cached entries. If omitted, defaults to 300 seconds (5 minutes).     |
| `max_size_mib`              | `Option<u64>` | Maximum cache size. If omitted, defaults to 10 MiB.                                  |

**Defaults**

- default_cache_ttl_seconds: 300 seconds (5 min)
- max_size_mib: 10 MiB

---

## `MozAdsPlacementRequest`

Describes a single ad placement to request from MARS. A vector of these are required for the `request_ads` method on the client.

```rust
pub struct MozAdsPlacementRequest {
  pub placement_id: String,
  pub iab_content: Option<IABContent>,
}
```

| Field          | Type                 | Description                                                                           |
| -------------- | -------------------- | ------------------------------------------------------------------------------------- |
| `placement_id` | `String`             | Unique identifier for the ad placement. Must be unique within one `request_ads` call. |
| `iab_content`  | `Option<IABContent>` | Optional IAB content classification for targeting.                                    |

**Validation Rules:**

- `placement_id` values must be unique per request.

---

## `MozAdsPlacementRequestWithCount`

Describes a single ad placement and the maximum number of ads to request for that placement. A vector of these is used by the `request_multiple_ads` method on the client.

```rust
pub struct MozAdsPlacementRequestWithCount {
  pub count: u32,
  pub placement_id: String,
  pub iab_content: Option<IABContent>,
}
```

---

## `MozAdsRequestOptions`

Options passed when making a single ad request.

```rust
pub struct MozAdsRequestOptions {
  pub cache_policy: Option<RequestCachePolicy>,
}
```

| Field          | Type                         | Description                                                                                    |
| -------------- | ---------------------------- | ---------------------------------------------------------------------------------------------- |
| `cache_policy` | `Option<RequestCachePolicy>` | Per-request caching policy. If `None`, uses the client’s default TTL with a `CacheFirst` mode. |

---

## `RequestCachePolicy`

Defines how each request interacts with the cache.

```rust
pub struct RequestCachePolicy {
  pub mode: CacheMode,
  pub ttl_seconds: Option<u64>,
}
```

| Field         | Type          | Description                                                                                                                |
| ------------- | ------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `mode`        | `CacheMode`   | Strategy for combining cache and network. Can be `CacheFirst` or `NetworkFirst`.                                           |
| `ttl_seconds` | `Option<u64>` | Optional per-request TTL override in seconds. `None` uses the client default. `Some(0)` disables caching for this request. |

---

## `CacheMode`

Determines how the cache is used during a request.

```rust
pub enum CacheMode {
  CacheFirst,
  NetworkFirst,
}
```

| Variant        | Behavior                                                                                           |
| -------------- | -------------------------------------------------------------------------------------------------- |
| `CacheFirst`   | Check cache first, return cached response if found, otherwise make a network request and store it. |
| `NetworkFirst` | Always fetch from network, then cache the result.                                                  |

---

## `MozAd`

The ad creative, callbacks, and metadata provided for each ad returned from MARS.

```rust
pub struct MozAd {
  pub url: String,
  pub image_url: String,
  pub format: String,
  pub block_key: String,
  pub alt_text: Option<String>,
  pub callbacks: AdCallbacks,
}
```

| Field       | Type             | Description                                 |
| ----------- | ---------------- | ------------------------------------------- |
| `url`       | `String`         | Destination URL.                            |
| `image_url` | `String`         | Creative asset URL.                         |
| `format`    | `String`         | Ad format e.g., `"skyscraper"`.             |
| `block_key` | `String`         | The block key generated for the advertiser. |
| `alt_text`  | `Option<String>` | Alt text if available.                      |
| `callbacks` | `AdCallbacks`    | Lifecycle callback endpoints.               |

---

## `AdCallbacks`

```rust
pub struct AdCallbacks {
  pub click: String,
  pub impression: String,
  pub report: Option<String>,
}
```

| Field        | Type             | Description              |
| ------------ | ---------------- | ------------------------ |
| `click`      | `String`         | Click callback URL.      |
| `impression` | `String`         | Impression callback URL. |
| `report`     | `Option<String>` | Report callback URL.     |

---

## `AdContentCategory`

Provides IAB content classification context for a placement.

```rust
pub struct AdContentCategory {
  pub taxonomy: IABContentTaxonomy,
  pub category_ids: Vec<String>,
}
```

| Field          | Type                 | Description                           |
| -------------- | -------------------- | ------------------------------------- |
| `taxonomy`     | `IABContentTaxonomy` | IAB taxonomy version.                 |
| `category_ids` | `Vec<String>`        | One or more IAB category identifiers. |

---

## `IABContentTaxonomy`

The [IAB Content Taxonomy](https://www.iab.com/guidelines/content-taxonomy/) version to be used in the request. e.g `IAB-1.0`

```rust
pub enum IABContentTaxonomy {
  IAB1_0,
  IAB2_0,
  IAB2_1,
  IAB2_2,
  IAB3_0,
}
```

> Note: The generated native bindings for the values may look different depending on the language (snake-case, camel case, etc.) as a result of UniFFI's formatting.

---

## Internal Cache Behavior

### Cache Overview

The internal HTTP cache is a SQLite-backed key-value store layered over viaduct::Request::send().
It reduces redundant network traffic and improves latency for repeated or identical ad requests.

### Cache Lifecycle

Each network response can be stored in the cache with an associated effective TTL, computed as:

```rust
effective_ttl = min(server_max_age, client_default_ttl, per_request_ttl)
```

where:

- `server_max_age` comes from the HTTP Cache-Control: max-age=N header (if present),
- `client_default_ttl` is set in `MozAdsCacheConfig`,
- `per_request_ttl` is an optional override set in `RequestCachePolicy`.

If the effective TTL resolves to 0 seconds, the response is not cached.

### Configuring The Cache

#### Example Client Configuration

```rust
// Swift / Kotlin pseudocode
let cache = MozAdsCacheConfig(
    dbPath: "/tmp/ads_cache.sqlite",
    defaultCacheTtlSeconds: 600,   // 10 min
    maxSizeMib: 20                 // 20 MiB
)

let clientCfg = MozAdsClientConfig(
    environment: .prod,
    cacheConfig: cache
)

let client = MozAdsClient.new(clientConfig: clientCfg)

```

Where `db_path` represents the location of the SQLite file. This must be a file that the client has permission to write to.

#### Example Request Policy Override

Assuming you have at least initialized the client with a `db_path`, individual requests can override caching behavior. However, recall the minimum TTL is always respected. So this override will only provide a new ttl floor.

```rust
// Always fetch from network but only cache for 60 seconds
let options = MozAdsRequestOptions(
    cachePolicy: RequestCachePolicy(mode: .networkFirst, ttlSeconds: 60)
)

// Use it when requesting ads
let placements = client.requestAds(configs, options: options)
```

### Cache Invalidation

**TTL-based expiry (automatic):**

At the start of each send, the cache computes a cutoff from chrono::Utc::now() - ttl and deletes rows older than that. This is a coarse, global freshness window that bounds how long entries can live.

**Size-based trimming (automatic):**
After storing a cacheable miss, the cache enforces max_size by deleting the oldest rows until the total stored size is ≤ the maximum allowed size of the cache. Due to the small size of items in the cache and the relatively short TTL, this behavior should be rare.

**Manual clearing (explicit):**
The cache can be manually cleared by the client using the exposed `client.clear_cache()` method. This clears _all_ objects in the cache.

---

### Example Usage

Under construction
