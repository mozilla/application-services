# Suggest

The API for the `SuggestStore` can be found in the [MozillaComponents Swift documentation](https://mozilla.github.io/application-services/swift/Classes/SuggestStore.html).

> Make sure you initialize [`viaduct`](../viaduct.md) for this component.

> The `SuggestStore` is a synchronous, which needs to be wrapped in the asynchronous primitive of the target language you are using it in.

## Setting up the store

You need to import one or more of the following primitives to work with the `SuggestStore` (these come from the generated `suggest.swift` file, produced by `uniffi`):

```swift
import class MozillaAppServices.SuggestStore
import class MozillaAppServices.SuggestStoreBuilder
import class MozillaAppServices.Viaduct
import enum MozillaAppServices.SuggestionProvider
import enum MozillaAppServices.RemoteSettingsServer
import struct MozillaAppServices.SuggestIngestionConstraints
import struct MozillaAppServices.SuggestionQuery
```

On start up of your application, you create a `SuggestStore` (as a singleton). You do this via the `SuggestStoreBuilder`, which returns a `SuggestStore`:

```swift
let store: SuggestStore

var builder = SuggestStoreBuilder()
    .dataPath(path: dataPath)

if let remoteSettingsServer {
    builder = builder.remoteSettingsServer(server: remoteSettingsServer)
}

store = try builder.build()
```

* You need to set the `dataPath`, which is the path (the SQLite location) where you store your suggestions.
* The `remoteSettingsServer` is only needed if you want to set the server to anything else but `prod`. If so, you pass a `RemoteSettingsServer` object.

## Ingesting suggestions

Ingesting suggestions happens in two different ways: On startup, and then, periodically, in the background.

* [`SuggestIngestionConstraints`](https://mozilla.github.io/application-services/kotlin/kotlin-components-docs/mozilla.appservices.suggest/-suggest-ingestion-constraints/index.html?query=data%20class%20SuggestIngestionConstraints(var%20providers:%20List%3CSuggestionProvider%3E?%20=%20null,%20var%20providerConstraints:%20SuggestionProviderConstraints?%20=%20null,%20var%20emptyOnly:%20Boolean%20=%20false)) is used to control what gets ingested.
* Use the `providers` field to limit ingestion by provider type.
* Use the `providerConstraints` field to add additional constraints, currently this is only used for exposure suggestions.


### On Start Up
Ingest with `SuggestIngestionConstraints::empty_only=true` shortly after each startup. This ensures we have something in the DB on the first run and also after upgrades where we often will clear the DB to start from scratch.

### Periodically

Ingest with `SuggestIngestionConstraints::empty_only=false` on regular schedule (like once a day).

Example:

```swift
try self.store.ingest(constraints: SuggestIngestionConstraints(
    emptyOnly: false,
    providers: [SuggestionProvider.AMP_MOBILE, SuggestionProvider.WIKIPEDIA, SuggestionProvider.WEATHER]
))
```

## Querying Suggestions

Call `SuggestStore::query` to fetch suggestions for the suggest bar. The `providers` parameter should be the same value that got passed to `ingest()`.

```swift
try self.store.query(query: SuggestionQuery(
    keyword: keyword,
    providers: [SuggestionProvider.AMP_MOBILE, SuggestionProvider.WIKIPEDIA, SuggestionProvider.WEATHER],
    limit: limit
))
```

## Interrupt querying

Call `SuggestStore::Interrupt` with `InterruptKind::Read` to interrupt any in-progress queries when the user cancels a query and before running the next query.

```swift
store.interrupt(kind: .readWrite)
```

## Shutdown the store

On shutdown, call `SuggestStore::Interrupt` with `InterruptKind::ReadWrite` to interrupt any in-progress ingestion and queries.