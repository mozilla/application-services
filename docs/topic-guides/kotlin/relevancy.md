# Relevancy

The `relevancy` component tracks the user's interests locally, without sharing any data over the network. The component currently supports building an interest vector based on the URLs they visit.

## Setting up the store

To use the `RelevancyStore` in Kotlin, you need to import the relevant classes and data types from the `MozillaAppServices` library:

```kotlin
import mozilla.appservices.relevancy.RelevancyStore
import mozilla.appservices.relevancy.InterestVector
```

To work with the `RelevancyStore`, you need to create an instance using a database path where the user’s interest data will be stored:

```kotlin
val store = RelevancyStore(dbPath)
```

* `dbPath`: This is the path to the SQLite database where the relevancy data is stored. The initialization is non-blocking, and the database is opened lazily.

## Ingesting relevancy data

To build the user's interest vector, call the `ingest` function with a list of URLs ranked by frequency. This method downloads the interest data, classifies the user's top URLs, and builds the interest vector. This process may take time and should only be called from a worker thread.

### Example usage of `ingest`:

```kotlin
val topUrlsByFrequency = listOf("https://example.com", "https://another-example.com")
val interestVector = store.ingest(topUrlsByFrequency)
```
* `topUrlsByFrequency`: A list of URLs ranked by how often and recently the user has visited them. This data is used to build the user's interest vector.
* The `ingest` function returns an `InterestVector`, which contains the user's interest levels for different tracked categories.

The ingestion process includes:
* Downloading the interest data from remote settings (eventually cached/stored in the database).
* Matching the user’s top URLs against the interest data.
* Storing the interest vector in the database.

> This method may execute for a long time and should only be called from a worker thread.

## Getting the user's interest vector

Once the user's interest vector has been built by ingestion, you can retrieve it using the `userInterestVector` function. This is useful for displaying the vector, for example, in an about page.

### Example usage of `userInterestVector`:

```kotlin
val interestVector = store.userInterestVector()
```
* This method returns an `InterestVector`, which is a record with a field that measures the interest for each category we track. The counts are not normalized.

## Interrupting ongoing operations

If the application is shutting down or you need to stop ongoing database queries, you can call `interrupt()` to stop any work that the `RelevancyStore` is doing.

### Example usage of `interrupt`:

```kotlin
store.interrupt()
```

* This interrupts any in-progress work, like ingestion or querying operations.

## Shutdown

Before shutting down the application, you should call `close()` to close the database and other open resources.

### Example usage of `close`:

```kotlin
store.close()
```

* This will close any open resources and interrupt any in-progress queries running on other threads.
