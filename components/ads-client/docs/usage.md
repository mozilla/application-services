
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
  pub fn new(db_path: String) -> Self
}
```

Creates a new client with a fresh context ID and initializes on-disk HTTP cache state at `db_path`.

#### Methods

| Method | Return Type | Description |
|--------|-------------|-------------|
| `request_ads(&self, moz_ad_configs: Vec<MozAdsPlacementConfig>)` | `AdsClientApiResult<HashMap<String, MozAdsPlacement>>` | Requests ads for the given placement configurations. Returns a map keyed by `placement_id`. |
| `record_impression(&self, placement: MozAdsPlacement)` | `AdsClientApiResult<()>` | Records an impression for the given placement (fires the ad’s impression callback). |
| `record_click(&self, placement: MozAdsPlacement)` | `AdsClientApiResult<()>` | Records a click for the given placement (fires the ad’s click callback). |
| `report_ad(&self, placement: MozAdsPlacement)` | `AdsClientApiResult<()>` | Reports the given placement (fires the ad’s report callback). |
| `cycle_context_id(&self)` | `AdsClientApiResult<String>` | Rotates the client’s context ID and returns the **previous** ID. |
| `clear_cache(&self)` | `AdsClientApiResult<()>` | Clears the client’s HTTP cache. Returns an error if clearing fails. |

> **Notes**
> - We recommend that this client be initialized as a singleton or something similar so that multiple instances of the client do not exist at once.
> - Responses from `request_ads` will omit placements with no fill. Those keys won’t appear in the returned map.
> - The HTTP cache implementation details are **not** exposed via UniFFI. Only `db_path` (constructor) and `clear_cache()` appear in the public surface.

---

## `MozAdsPlacementConfig`

Describes a single ad placement to request from MARS. A vector of these are required for the `request_ads` method on the client.

```rust
pub struct MozAdsPlacementConfig {
  pub placement_id: String,
  pub iab_content: Option<IABContent>,
}
```

| Field | Type | Description |
|------|------|-------------|
| `placement_id` | `String` | Unique identifier for the ad placement. Must be unique within one `request_ads` call. |
| `iab_content` | `Option<IABContent>` | Optional IAB content classification for targeting. |

**Validation Rules:**
- `placement_id` values must be unique per request.

---

## `MozAdsPlacement`

Represents a served ad placement and its content.

```rust
pub struct MozAdsPlacement {
  pub placement_config: MozAdsPlacementConfig,
  pub content: MozAd,
}
```

| Field | Type | Description |
|------|------|-------------|
| `placement_config` | `MozAdsPlacementConfig` | The configuration used to request this ad. |
| `content` | `MozAd` | The ad creative and its callbacks. |


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

| Field | Type | Description |
|------|------|-------------|
| `url` | `String` | Destination URL. |
| `image_url` | `String` | Creative asset URL. |
| `format` | `String` | Ad format e.g., `"skyscraper"`. |
| `block_key` | `String` | The block key generated for the advertiser. |
| `alt_text` | `Option<String>` | Alt text if available. |
| `callbacks` | `AdCallbacks` | Lifecycle callback endpoints. |


---

## `AdCallbacks`

```rust
pub struct AdCallbacks {
  pub click: Option<String>,
  pub impression: Option<String>,
  pub report: Option<String>,
}
```

| Field | Type | Description |
|------|------|-------------|
| `click` | `Option<String>` | Click callback URL. |
| `impression` | `Option<String>` | Impression callback URL. |
| `report` | `Option<String>` | Report callback URL. |


---

## `AdContentCategory`

Provides IAB content classification context for a placement.

```rust
pub struct AdContentCategory {
  pub taxonomy: IABContentTaxonomy,
  pub category_ids: Vec<String>,
}
```

| Field | Type | Description |
|------|------|-------------|
| `taxonomy` | `IABContentTaxonomy` | IAB taxonomy version. |
| `category_ids` | `Vec<String>` | One or more IAB category identifiers. |

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

Internal to the Ads-Client component is a lightweight, SQLite-backed HTTP cache that sits in front of `viaduct::Request::send()`. This cache is used for two primary purposes: preventing duplicate network requests from being made over some interval, and "pre-fetching" of ads that can be stored and used without needing to wait for a network request to finish.

> **Note**: The cache is managed automatically "under-the-hood" by the `ads-client` component with no additional input needed form the client when making an ad request. All the caching logic is handled during the call to `request_ads(...)`.

### Configuring The Cache

Currently, the only external configuration we expose for the cache is when constructing `MozAdsClient`:

```rust
impl MozAdsClient {
  pub fn new(db_path: String) -> Self
}
```

Where `db_path` represents the location of the SQLite file. This must be a file that the client has permission to write to.

### Cache Invalidation

**TTL-based expiry (automatic):**

At the start of each send, the cache computes a cutoff from chrono::Utc::now() - ttl and deletes rows older than that. This is a coarse, global freshness window that bounds how long entries can live.

**Size-based trimming (automatic):**
After storing a cacheable miss, the cache enforces max_size by deleting the oldest rows until the total stored size is ≤ the maximum allowed size of the cache. Due to the small size of items in the cache and the relatively short TTL, this behavior should be rare.

**Manual clearing (explicit):**
The cache can be manually cleared by the client using the exposed `client.clear_cache()` method. This clears *all* objects in the cache.

---

### Example Usage

Under construction
