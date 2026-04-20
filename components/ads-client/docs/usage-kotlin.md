# Mozilla Ads Client (MAC) — Kotlin API Reference

## Overview

This document covers the full API reference for the `ads_client` component, with all examples and type definitions in Kotlin.
It includes every type and function exposed via UniFFI that is part of the public API surface.

---

## `MozAdsClient`

Top-level client object for requesting ads and recording lifecycle events.

```kotlin
class MozAdsClient {
    // No public fields — use MozAdsClientBuilder to create instances
}
```

#### Creating a Client

Use the `MozAdsClientBuilder` to configure and create the client. The builder provides a fluent API for setting configuration options.

```kotlin
val client = MozAdsClientBuilder()
    .environment(MozAdsEnvironment.PROD)
    .cacheConfig(cache)
    .telemetry(telemetry)
    .build()
```

#### Methods

| Method                                                                                                                  | Return Type                                            | Description                                                                                                                                                                          |
| ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `clearCache()`                                                                                                          | `Unit`                                                 | Clears the client's HTTP cache. Throws on failure.                                                                                                                                   |
| `recordClick(clickUrl: String, options: MozAdsCallbackOptions?)`                                                                                         | `Unit`                                                 | Records a click using the provided callback URL (typically from `ad.callbacks.click`).                                                                                               |
| `recordImpression(impressionUrl: String, options: MozAdsCallbackOptions?)`                                                                               | `Unit`                                                 | Records an impression using the provided callback URL (typically from `ad.callbacks.impression`).                                                                                    |
| `reportAd(reportUrl: String, reason: MozAdsReportReason, options: MozAdsCallbackOptions?)`                                                                                           | `Unit`                                                 | Reports an ad using the provided callback URL (typically from `ad.callbacks.report`).                                                                                                |
| `requestImageAds(mozAdRequests: List<MozAdsPlacementRequest>, options: MozAdsRequestOptions?)`                          | `Map<String, MozAdsImage>`                             | Requests one image ad per placement. Optional `MozAdsRequestOptions` can adjust caching behavior. Returns a map keyed by `placementId`.                                              |
| `requestSpocAds(mozAdRequests: List<MozAdsPlacementRequestWithCount>, options: MozAdsRequestOptions?)`                  | `Map<String, List<MozAdsSpoc>>`                        | Requests spoc ads per placement. Each placement request specifies its own count. Optional `MozAdsRequestOptions` can adjust caching behavior. Returns a map keyed by `placementId`.  |
| `requestTileAds(mozAdRequests: List<MozAdsPlacementRequest>, options: MozAdsRequestOptions?)`                           | `Map<String, MozAdsTile>`                              | Requests one tile ad per placement. Optional `MozAdsRequestOptions` can adjust caching behavior. Returns a map keyed by `placementId`.                                               |

> **Notes**
>
> - We recommend that this client be initialized as a singleton or something similar so that multiple instances of the client do not exist at once.
> - Responses omit placements with no fill. Empty placements do not appear in the returned maps.
> - The HTTP cache is internally managed. Configuration can be set with `MozAdsClientBuilder`. Per-request cache settings can be set with `MozAdsRequestOptions`.
> - If `cacheConfig` is `null`, caching is disabled entirely.

---

## `MozAdsClientBuilder`

Builder for configuring and creating the ads client. Use the fluent builder pattern to set configuration options.

```kotlin
class MozAdsClientBuilder {
    fun environment(environment: MozAdsEnvironment): MozAdsClientBuilder
    fun cacheConfig(cacheConfig: MozAdsCacheConfig): MozAdsClientBuilder
    fun telemetry(telemetry: MozAdsTelemetry): MozAdsClientBuilder
    fun build(): MozAdsClient
}
```

#### Methods

- **`MozAdsClientBuilder()`** - Creates a new builder with default values
- **`environment(environment: MozAdsEnvironment)`** - Sets the MARS environment (Prod, Staging, or Test)
- **`cacheConfig(cacheConfig: MozAdsCacheConfig)`** - Sets the cache configuration
- **`telemetry(telemetry: MozAdsTelemetry)`** - Sets the telemetry implementation
- **`build()`** - Builds and returns the configured client

| Configuration  | Type                  | Description                                                                                            |
| -------------- | --------------------- | ------------------------------------------------------------------------------------------------------ |
| `environment`  | `MozAdsEnvironment`   | Selects which MARS environment to connect to. Unless in a dev build, this value can only ever be Prod. Defaults to Prod. |
| `cacheConfig`  | `MozAdsCacheConfig?`  | Optional configuration for the internal cache.                                                         |
| `telemetry`    | `MozAdsTelemetry?`    | Optional telemetry instance for recording metrics. If not provided, a no-op implementation is used.    |

---

## `MozAdsTelemetry`

Telemetry interface for recording ads client metrics. You must provide an implementation of this interface to the `MozAdsClientBuilder` to enable telemetry collection. If no telemetry instance is provided, a no-op implementation is used and no metrics will be recorded.

```kotlin
interface MozAdsTelemetry {
    fun recordBuildCacheError(label: String, value: String)
    fun recordClientError(label: String, value: String)
    fun recordClientOperationTotal(label: String)
    fun recordDeserializationError(label: String, value: String)
    fun recordHttpCacheOutcome(label: String, value: String)
}
```

#### Implementation Example

```kotlin
import mozilla.appservices.adsclient.MozAdsTelemetry
import org.mozilla.appservices.ads_client.GleanMetrics.AdsClient

class AdsClientTelemetry : MozAdsTelemetry {
    override fun recordBuildCacheError(label: String, value: String) {
        AdsClient.buildCacheError[label].set(value)
    }

    override fun recordClientError(label: String, value: String) {
        AdsClient.clientError[label].set(value)
    }

    override fun recordClientOperationTotal(label: String) {
        AdsClient.clientOperationTotal[label].add()
    }

    override fun recordDeserializationError(label: String, value: String) {
        AdsClient.deserializationError[label].set(value)
    }

    override fun recordHttpCacheOutcome(label: String, value: String) {
        AdsClient.httpCacheOutcome[label].set(value)
    }
}
```

---

## `MozAdsCacheConfig`

Describes the behavior and location of the on-disk HTTP cache.

```kotlin
data class MozAdsCacheConfig(
    val dbPath: String,
    val defaultCacheTtlSeconds: Long?,
    val maxSizeMib: Long?
)
```

| Field                       | Type    | Description                                                                          |
| --------------------------- | ------- | ------------------------------------------------------------------------------------ |
| `dbPath`                    | `String`| Path to the SQLite database file used for cache storage. Required to enable caching. |
| `defaultCacheTtlSeconds`    | `Long?` | Default TTL for cached entries. If omitted, defaults to 300 seconds (5 minutes).     |
| `maxSizeMib`                | `Long?` | Maximum cache size. If omitted, defaults to 10 MiB.                                  |

**Defaults**

- defaultCacheTtlSeconds: 300 seconds (5 min)
- maxSizeMib: 10 MiB

#### Configuration Example

```kotlin
val cache = MozAdsCacheConfig(
    dbPath = "/tmp/ads_cache.sqlite",
    defaultCacheTtlSeconds = 600L,   // 10 min
    maxSizeMib = 20L                 // 20 MiB
)

val telemetry = AdsClientTelemetry()

val client = MozAdsClientBuilder()
    .environment(MozAdsEnvironment.PROD)
    .cacheConfig(cache)
    .telemetry(telemetry)
    .build()
```

---

## `MozAdsPlacementRequest`

Describes a single ad placement to request from MARS. A list of these is required for the `requestImageAds` and `requestTileAds` methods on the client.

```kotlin
data class MozAdsPlacementRequest(
    val placementId: String,
    val iabContent: MozAdsIABContent?
)
```

| Field          | Type                | Description                                                                     |
| -------------- | ------------------- | ------------------------------------------------------------------------------- |
| `placementId`  | `String`            | Unique identifier for the ad placement. Must be unique within one request call. |
| `iabContent`   | `MozAdsIABContent?` | Optional IAB content classification for targeting.                              |

**Validation Rules:**

- `placementId` values must be unique per request.

---

## `MozAdsPlacementRequestWithCount`

Describes a single ad placement to request from MARS with a count parameter. A list of these is required for the `requestSpocAds` method on the client.

```kotlin
data class MozAdsPlacementRequestWithCount(
    val count: Int,
    val placementId: String,
    val iabContent: MozAdsIABContent?
)
```

| Field          | Type                | Description                                                                     |
| -------------- | ------------------- | ------------------------------------------------------------------------------- |
| `count`        | `Int`               | Number of spoc ads to request for this placement.                               |
| `placementId`  | `String`            | Unique identifier for the ad placement. Must be unique within one request call. |
| `iabContent`   | `MozAdsIABContent?` | Optional IAB content classification for targeting.                              |

**Validation Rules:**

- `placementId` values must be unique per request.

---

## `MozAdsRequestOptions`

Options passed when making a single ad request.

```kotlin
data class MozAdsRequestOptions(
    val cachePolicy: MozAdsRequestCachePolicy?,
    val ohttp: Boolean = false
)
```

| Field          | Type                         | Description                                                                                    |
| -------------- | ---------------------------- | ---------------------------------------------------------------------------------------------- |
| `cachePolicy`  | `MozAdsRequestCachePolicy?`  | Per-request caching policy. If `null`, uses the client's default TTL with a `CacheFirst` mode. |
| `ohttp`        | `Boolean`                    | Whether to route this request through OHTTP. Defaults to `false`.                              |

---

## `MozAdsCallbackOptions`

Options passed when making callback requests (click, impression, report).

```kotlin
data class MozAdsCallbackOptions(
    val ohttp: Boolean = false
)
```

| Field   | Type      | Description                                                        |
| ------- | --------- | ------------------------------------------------------------------ |
| `ohttp` | `Boolean` | Whether to route this callback through OHTTP. Defaults to `false`. |

#### OHTTP Usage Example

```kotlin
// Request ads over OHTTP
val ads = client.requestTileAds(placements, MozAdsRequestOptions(ohttp = true))

// Record a click over OHTTP
client.recordClick(ad.callbacks.click, MozAdsCallbackOptions(ohttp = true))

// Record an impression over OHTTP
client.recordImpression(ad.callbacks.impression, MozAdsCallbackOptions(ohttp = true))
```

> **Note:** OHTTP must be configured at the viaduct level before use. When `ohttp` is `true`, the client automatically performs a preflight request to obtain geo-location and user-agent headers, which are injected into the MARS request.

---

## `MozAdsRequestCachePolicy`

Defines how each request interacts with the cache.

```kotlin
data class MozAdsRequestCachePolicy(
    val mode: MozAdsCacheMode,
    val ttlSeconds: Long?
)
```

| Field         | Type              | Description                                                                                                              |
| ------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `mode`        | `MozAdsCacheMode` | Strategy for combining cache and network. Can be `CACHE_FIRST` or `NETWORK_FIRST`.                                      |
| `ttlSeconds`  | `Long?`           | Optional per-request TTL override in seconds. `null` uses the client default. `0` disables caching for this request.     |

#### Per-Request Cache Policy Override Example

```kotlin
// Always fetch from network but only cache for 60 seconds
val options = MozAdsRequestOptions(
    cachePolicy = MozAdsRequestCachePolicy(mode = MozAdsCacheMode.NETWORK_FIRST, ttlSeconds = 60L)
)

// Use it when requesting ads
val placements = client.requestImageAds(configs, options = options)
```

---

## `MozAdsCacheMode`

Determines how the cache is used during a request.

```kotlin
enum class MozAdsCacheMode {
    CACHE_FIRST,
    NETWORK_FIRST
}
```

| Variant         | Behavior                                                                                           |
| --------------- | -------------------------------------------------------------------------------------------------- |
| `CACHE_FIRST`   | Check cache first, return cached response if found, otherwise make a network request and store it. |
| `NETWORK_FIRST` | Always fetch from network, then cache the result.                                                  |

---

## `MozAdsImage`

The image ad creative, callbacks, and metadata provided for each image ad returned from MARS.

```kotlin
data class MozAdsImage(
    val altText: String?,
    val blockKey: String,
    val callbacks: MozAdsCallbacks,
    val format: String,
    val imageUrl: String,
    val url: String
)
```

| Field       | Type              | Description                                 |
| ----------- | ----------------- | ------------------------------------------- |
| `url`       | `String`          | Destination URL.                            |
| `imageUrl`  | `String`          | Creative asset URL.                         |
| `format`    | `String`          | Ad format e.g., `"skyscraper"`.             |
| `blockKey`  | `String`          | The block key generated for the advertiser. |
| `altText`   | `String?`         | Alt text if available.                      |
| `callbacks` | `MozAdsCallbacks` | Lifecycle callback endpoints.               |

---

## `MozAdsSpoc`

The spoc ad creative, callbacks, and metadata provided for each spoc ad returned from MARS.

```kotlin
data class MozAdsSpoc(
    val blockKey: String,
    val callbacks: MozAdsCallbacks,
    val caps: MozAdsSpocFrequencyCaps,
    val domain: String,
    val excerpt: String,
    val format: String,
    val imageUrl: String,
    val ranking: MozAdsSpocRanking,
    val sponsor: String,
    val sponsoredByOverride: String?,
    val title: String,
    val url: String
)
```

| Field                   | Type                      | Description                                 |
| ----------------------- | ------------------------- | ------------------------------------------- |
| `url`                   | `String`                  | Destination URL.                            |
| `imageUrl`              | `String`                  | Creative asset URL.                         |
| `format`                | `String`                  | Ad format e.g., `"spoc"`.                   |
| `blockKey`              | `String`                  | The block key generated for the advertiser. |
| `title`                 | `String`                  | Spoc ad title.                              |
| `excerpt`               | `String`                  | Spoc ad excerpt/description.                |
| `domain`                | `String`                  | Domain of the spoc ad.                      |
| `sponsor`               | `String`                  | Sponsor name.                               |
| `sponsoredByOverride`   | `String?`                 | Optional override for sponsor name.         |
| `caps`                  | `MozAdsSpocFrequencyCaps` | Frequency capping information.              |
| `ranking`               | `MozAdsSpocRanking`       | Ranking and personalization information.    |
| `callbacks`             | `MozAdsCallbacks`         | Lifecycle callback endpoints.               |

---

## `MozAdsTile`

The tile ad creative, callbacks, and metadata provided for each tile ad returned from MARS.

```kotlin
data class MozAdsTile(
    val blockKey: String,
    val callbacks: MozAdsCallbacks,
    val format: String,
    val imageUrl: String,
    val name: String,
    val url: String
)
```

| Field       | Type              | Description                                 |
| ----------- | ----------------- | ------------------------------------------- |
| `url`       | `String`          | Destination URL.                            |
| `imageUrl`  | `String`          | Creative asset URL.                         |
| `format`    | `String`          | Ad format e.g., `"tile"`.                   |
| `blockKey`  | `String`          | The block key generated for the advertiser. |
| `name`      | `String`          | Tile ad name.                               |
| `callbacks` | `MozAdsCallbacks` | Lifecycle callback endpoints.               |

---

## `MozAdsSpocFrequencyCaps`

Frequency capping information for spoc ads.

```kotlin
data class MozAdsSpocFrequencyCaps(
    val capKey: String,
    val day: Int
)
```

| Field    | Type     | Description                       |
| -------- | -------- | --------------------------------- |
| `capKey` | `String` | Frequency cap key identifier.     |
| `day`    | `Int`    | Day number for the frequency cap. |

---

## `MozAdsSpocRanking`

Ranking and personalization information for spoc ads.

```kotlin
data class MozAdsSpocRanking(
    val priority: Int,
    val personalizationModels: Map<String, Int>,
    val itemScore: Double
)
```

| Field                    | Type                | Description                   |
| ------------------------ | ------------------- | ----------------------------- |
| `priority`               | `Int`               | Priority score for ranking.   |
| `personalizationModels`  | `Map<String, Int>`  | Personalization model scores. |
| `itemScore`              | `Double`            | Overall item score.           |

---

## `MozAdsCallbacks`

```kotlin
data class MozAdsCallbacks(
    val click: String,
    val impression: String,
    val report: String?
)
```

| Field        | Type      | Description              |
| ------------ | --------- | ------------------------ |
| `click`      | `String`  | Click callback URL.      |
| `impression` | `String`  | Impression callback URL. |
| `report`     | `String?` | Report callback URL.     |

---

## `MozAdsIABContent`

Provides IAB content classification context for a placement.

```kotlin
data class MozAdsIABContent(
    val taxonomy: MozAdsIABContentTaxonomy,
    val categoryIds: List<String>
)
```

| Field          | Type                       | Description                           |
| -------------- | -------------------------- | ------------------------------------- |
| `taxonomy`     | `MozAdsIABContentTaxonomy` | IAB taxonomy version.                 |
| `categoryIds`  | `List<String>`             | One or more IAB category identifiers. |

---

## `MozAdsIABContentTaxonomy`

The [IAB Content Taxonomy](https://www.iab.com/guidelines/content-taxonomy/) version to be used in the request. e.g `IAB-1.0`

```kotlin
enum class MozAdsIABContentTaxonomy {
    IAB1_0,
    IAB2_0,
    IAB2_1,
    IAB2_2,
    IAB3_0
}
```

> Note: The generated UniFFI bindings use screaming snake-case for enum variants in Kotlin.

---

## Internal Cache Behavior

### Cache Overview

The internal HTTP cache is a SQLite-backed key-value store layered over the HTTP request layer.
It reduces redundant network traffic and improves latency for repeated or identical ad requests.

### Cache Lifecycle

Each network response can be stored in the cache with an associated effective TTL, computed as:

```
effective_ttl = min(server_max_age, client_default_ttl, per_request_ttl)
```

where:

- `server_max_age` comes from the HTTP `Cache-Control: max-age=N` header (if present),
- `client_default_ttl` is set in `MozAdsCacheConfig`,
- `per_request_ttl` is an optional override set in `MozAdsRequestCachePolicy`.

If the effective TTL resolves to 0 seconds, the response is not cached.

### Configuring The Cache

```kotlin
val cache = MozAdsCacheConfig(
    dbPath = "/tmp/ads_cache.sqlite",
    defaultCacheTtlSeconds = 600L,   // 10 min
    maxSizeMib = 20L                 // 20 MiB
)

val telemetry = AdsClientTelemetry()

val client = MozAdsClientBuilder()
    .environment(MozAdsEnvironment.PROD)
    .cacheConfig(cache)
    .telemetry(telemetry)
    .build()
```

Where `dbPath` represents the location of the SQLite file. This must be a file that the client has permission to write to.

### Cache Invalidation

**TTL-based expiry (automatic):**

At the start of each send, the cache computes a cutoff from the current time minus the TTL and deletes rows older than that. This is a coarse, global freshness window that bounds how long entries can live.

**Size-based trimming (automatic):**
After storing a cacheable miss, the cache enforces `maxSizeMib` by deleting the oldest rows until the total stored size is at or below the maximum allowed size of the cache. Due to the small size of items in the cache and the relatively short TTL, this behavior should be rare.

**Manual clearing (explicit):**
The cache can be manually cleared by the client using the exposed `client.clearCache()` method. This clears _all_ objects in the cache.
