# Suggest

The API for the `SuggestStore` can be found in the [MozillaComponents Kotlin documentation](https://mozilla.github.io/application-services/kotlin/kotlin-components-docs/mozilla.appservices.suggest/-suggest-store/index.html).

> Make sure you initialize [`viaduct`](../viaduct.md) for this component.

> The `SuggestStore` is a synchronous, which needs to be wrapped in the asynchronous primitive of the target language you are using it in.

## Setting up the store

You need to import one or more of the following primitives to work with the `SuggestStore` (these come from the generated `suggest.kt` file, produced by `uniffi`):

```kotlin
import mozilla.appservices.remotesettings.RemoteSettingsServer
import mozilla.appservices.suggest.SuggestApiException
import mozilla.appservices.suggest.SuggestIngestionConstraints
import mozilla.appservices.suggest.SuggestStore
import mozilla.appservices.suggest.SuggestStoreBuilder
import mozilla.appservices.suggest.Suggestion
import mozilla.appservices.suggest.SuggestionQuery
```

Create a `SuggestStore` as a singleton. You do this via the `SuggestStoreBuilder`, which returns a `SuggestStore`.  No I/O or network requests are performed during construction, which makes this safe to do at any point in the application startup:

```kotlin
internal val store: SuggestStore = {
    SuggestStoreBuilder()
        .dataPath(context.getDatabasePath(DATABASE_NAME).absolutePath)
        .remoteSettingsServer(remoteSettingsServer)
        .build()
```

* You need to set the `dataPath`, which is the path (the SQLite location) where you store your suggestions.
* The `remoteSettingsServer` is only needed if you want to set the server to anything else but `prod`. If so, you pass a `RemoteSettingsServer` object. 

## Ingesting suggestions

Ingesting suggestions happens in two different ways: On startup, and then, periodically, in the background.

* [`SuggestIngestionConstraints`](https://mozilla.github.io/application-services/kotlin/kotlin-components-docs/mozilla.appservices.suggest/-suggest-ingestion-constraints/index.html?query=data%20class%20SuggestIngestionConstraints(var%20providers:%20List%3CSuggestionProvider%3E?%20=%20null,%20var%20providerConstraints:%20SuggestionProviderConstraints?%20=%20null,%20var%20emptyOnly:%20Boolean%20=%20false) is used to control what gets ingested.
* Use the `providers` field to limit ingestion by provider type.
* Use the `providerConstraints` field to add additional constraints, currently this is only used for exposure suggestions.

### On Start Up
Ingest with `SuggestIngestionConstraints(emptyOnly=true)` shortly after each startup. This ensures we have something in the DB on the first run and also after upgrades where we often will clear the DB to start from scratch.

```kotlin
store.value.ingest(SuggestIngestionConstraints(emptyOnly = true, providers = listOf(SuggestionProvider.AMP_MOBILE, SuggestionProvider.WIKIPEDIA, SuggestionProvider.WEATHER)))
```

### Periodically

Ingest with `SuggestIngestionConstraints(emptyOnly=false)` on regular schedule (like once a day).

Example:

```kotlin
store.value.ingest(SuggestIngestionConstraints(emptyOnly = false))
```

## Querying Suggestions

Call `SuggestStore::query` to fetch suggestions for the suggest bar. The `providers` parameter should be the same value that got passed to `ingest()`.

```kotlin
store.value.query(
    SuggestionQuery(
        keyword = text,
        providers = listOf(SuggestionProvider.AMP_MOBILE, SuggestionProvider.WIKIPEDIA, SuggestionProvider.WEATHER),
        limit = MAX_NUM_OF_FIREFOX_SUGGESTIONS,
    ),
)
```

## Interrupt querying

Call `SuggestStore::Interrupt` with `InterruptKind::Read` to interrupt any in-progress queries when the user cancels a query and before running the next query.

```kotlin
store.value.interrupt()
```

## Shutdown the store

On shutdown, call `SuggestStore::Interrupt` with `InterruptKind::ReadWrite` to interrupt any in-progress ingestion and queries.