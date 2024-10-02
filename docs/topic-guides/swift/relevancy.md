# Relevancy

The `relevancy` component tracks the user's interests locally, without sharing any data over the network. The component currently supports building an interest vector based on the URLs they visit.

## Setting up the store

You need to import the following components to work with the `RelevancyStore` (these come from the generated `relevancy.swift` file, produced by `uniffi`):

```swift
import class MozillaAppServices.RelevancyStore
import struct MozillaAppServices.InterestVector
```

On startup of your application, you create a `RelevancyStore`. The store is initialized with a database path where the user’s interest vector will be stored:

```swift
let store = try RelevancyStore(dbPath: pathToDatabase)
```

* `dbPath`: This is the path to the SQLite database where the relevancy data is stored. The initialization is non-blocking, and the database is opened lazily.

## Ingesting relevancy data

Ingesting user data into the `RelevancyStore` builds the user's interest vector based on the top URLs they visit (measured by frequency). This should be called soon after startup but does not need to be scheduled periodically.

### Example usage of `ingest`:

```swift
let topUrlsByFrequency: [String] = ["https://example.com", "https://another-example.com"]
try store.ingest(topUrlsByFrequency: topUrlsByFrequency)
```
* `topUrlsByFrequency`: A list of URLs ranked by how often and recently the user has visited them. This data is used to build the user's interest vector.
* The `ingest` function returns an `InterestVector`, which contains the user's interest levels for different tracked categories.

The ingestion process includes:
* Downloading the interest data from remote settings (eventually cached/stored in the database).
* Matching the user’s top URLs against the interest data.
* Storing the interest vector in the database.

> This method may execute for a long time and should only be called from a worker thread.

## Getting the user's interest vector

After ingestion, you can retrieve the user's interest vector directly. This is useful for displaying the vector on an `about:` page or using it in other features.

### Example usage of `userInterestVector`:

```swift
let interestVector = try store.userInterestVector()
```
* This method returns an `InterestVector`, which is a record with a field that measures the interest for each category we track. The counts are not normalized.

## Interrupting ongoing operations

If the application is shutting down or you need to stop ongoing database queries, you can call `interrupt()` to stop any work that the `RelevancyStore` is doing.

### Example usage of `interrupt`:

```swift
store.interrupt()
```

* This interrupts any in-progress work, like ingestion or querying operations.

## Shutdown

Before shutting down the application, you should call `close()` to close the database and other open resources.

### Example usage of `close`:

```swift
store.close()
```

* This will close any open resources and interrupt any in-progress queries running on other threads.
