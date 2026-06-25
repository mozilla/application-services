# Using Async Rust

* Status: draft
* Deciders: 
* Date: ???
* Previous discussion: https://github.com/mozilla/application-services/pull/5910

## Context and Problem Statement

Our Rust components are currently written using synchronous Rust, however all current consumers wrap them in another class to present an async-style interface:

* Firefox Android uses `CoroutineScope.runBlocking` to adapt methods to be `suspend`
* Firefox iOs mostly uses `DispatchQueue` and completion handlers, but the long-term plan is to transition to `async` functions.
  Some components already do this, using `withCheckedThrowingContinuation` on top of `DispatchQueue` to define async functions.
* Firefox Desktop auto-generates C++ wrapper code to make them async using a [TOML config file](https://searchfox.org/mozilla-central/rev/cdfe21b20eacfaa6712dd9821d6383859ce386c6/toolkit/components/uniffi-bindgen-gecko-js/config.toml).
  This was chosen because JS is single-threaded and doesn't provide a simple way to run blocking functions in a task queue.
  One drawback from this choice is that Desktop has a very different async system compared to the mobile apps.

This presents some issues for Rust component development:

* Async is handled differently in JS vs Kotlin+Swift, which can be confusing for developers.
* Core component functionality ends up being implemented in wrapper code which means it needs to be implemented multiple times or some platforms will be missing features.
  For example, the Kotlin functionality to interrupt pending `query()` calls would also be useful on Swift, but we never implemented that .
* The code lives in other repos even though it logically belongs in app-services.
  This makes it more likely to go stale and be forgotten about.
* It makes our documentation system worse.  For example, our docs for [query()](https://mozilla.github.io/application-services/kotlin/kotlin-components-docs/mozilla.appservices.suggest/-suggest-store/query.html) say it's a sync function, but it practice it's actually async.

With the new UniFFI async capabilities, it's possible to move the async code into Rust and avoid this wrapper layer.
This ADR discusses if we should do this, how we could implement it, and what our general approach to async should be.
To make things concrete, the Suggest component is used as an example of what would change if we moved to async Rust.

### Scope

This ADR covers the question of using a wrapper layer to implant async functionality.
It does not cover some related questions:

* **Scheduling this work.**
  If we decide to embrace async Rust, we do not need to commit to any particular deadline for switching to it.
* **Wrappers in general.**
  [The previous PR](https://github.com/mozilla/application-services/pull/5910/) dedicated a separate ADR for this question, but we never came to a decision on this.
  It seems that there's no single answer to the question, the general consensus was that we should evaluate things on a case-by-case basis and this ADR will just focus on the case of async.
* **Wrappers in consumer code.** 
  If we change things so that the current async wrapping layer is no longer needed, consumer engineers will still have a choice on if they want to keep their current wrapper layer or not.
  This choice should be left to consumer engineers.
  This ADR will touch on this, but not recommend anything.

## Considered Options

* **Option A: Keep the Rust code sync, but move it.** 
  * Keep the Rust code sync, but move the wrapper code closer to the Rust code to address the code-ownership issues.
  * Move most functionality from the wrapper code into Rust.
    For example, error-handling code and the code to interrupt pending queries could all happen in Rust.
    This makes it harder for developers to forget about this functionality when writing Rust code.
  * The wrapper code would be responsible for wrapping sync calls to be async and basically nothing else.
* **Option B: Async Rust using the current UniFFI.**
  Make our component methods async by requiring the foreign code to pass in an interface that runs tasks in a worker queue and using
`oneshot::Channel` to communicate the results back.  See below for how this would work in practice.
  This allows us to switch to async Rust code without making any changes to the UniFFI core.
* **Option C: Async Rust with additional UniFFI support.**
  Like `B`, but make `WorkerQueue` and `RustTask` built-in UniFFI types.
  This would eliminate the need to define our own interfaces for these, instead UniFFI would allow foreign code to passing in `DispatchQueue`/`CoroutineScopes` to Rust and Rust could use those to run blocking tasks in a work queue.
* **Option D: Extend the uniffi-bindgen-gecko-js config system to Mobile.**
  Extend the Gecko-JS system, where the config specifies that functions/methods should be wrapped as async, to also work for Kotlin/Swift.

### Example code

I (Ben) made some diffs to illustrate how the code would change for each option.
When doing that, I realized the wrapper layer was actually implementing important functionality for the component:

* The Kotlin code extended our interrupt support to also interrupt pending `query()` calls.
* The Kotlin code also catches all errors and coverts them into regular returns (`query()` returns an empty list and `ingest()` returns false).
* The Swift code split async methods into 2 categories: low-priority calls like `ingest()` and high-priority calls like `query()`

As part of the example changes, I moved this functionality to our Rust components.
This results in some extra code in the diffs, but I thought it was good to explore the messy details of this transition.
A important factor for deciding this ADR is where we want this functionality to live.

#### Option B
  - [app-services changes](./files/0009/option-b-app-services.diff)
  - [android-components changes](./files/0009/option-b-android-components.diff)
  - [Firefox iOS changes](./files/0009/option-b-firefox-ios.diff)
  - [Firefox Desktop changes](./files/0009/option-b-firefox-desktop.diff)

This option is possible to implement today, so I was able to test that the app-services and android-components changes actually compiled
I didn't do that for iOS and desktop, mostly because it's harder to perform a local build.
I think we can be confident that the actual changes would look similar to this.

Summary of changes:
* Added the `WorkerQueue` and `RustTask` interfaces
  * `RustTask` encapsulates a single task
  * `WorkerQueue` is implemented by the foreign side and runs a `RustTask` in a work queue where it's okay to block.
* Use the above interfaces to wrap sync calls to be async, by running them in the `WorkerQueue` then sending the result via a `oneshot::Channel` that the 
  original async function was `await`ing.
  * This is also a good place to do the error conversion/reporting.
* SuggestStoreBuilder gets a `build_async` method, which creates an `SuggestStoreAsync`.
  It inputs 2 `WorkerQueue`s: one for ingest and one for everything else.
* Added supporting code so that `query()` can create it's `SqlInterruptScope` ahead of time, outside of the scheduled task.
  That way `interrupt()` can also interrupt pending calls to `query()`.
* Updated the `ingest()` method to catch errors and return `false` rather than propagating them.
  In general, I think this is a better API for consumers since there's nothing meaningful they can do with errors other than report them.
  `query()` should probably also be made infallible.
* Removed the wrapper class from `android-components`, but kept the wrapper class in `firefox-ios`.
  The goal here was to show the different options for consumer code.

#### Option C
  - [app-services changes](./files/0009/option-c-app-services.diff)
  - [android-components changes](./files/0009/option-c-android-components.diff)
  - [Firefox iOS changes](./files/0009/option-c-firefox-ios.diff)
  - [Firefox Desktop changes](./files/0009/option-c-firefox-desktop.diff)

This option assumes that UniFFI will provide something similar to `WorkerQueue`, so we don't need to define/implement that in app-services or the consumer repos.
This requires changes to UniFFI core, so none of this code works today.
However, I think we can be fairly confident that these changes will work since we have a long-standing [UniFFI PR](https://github.com/mozilla/uniffi-rs/pull/1837) that implements a similar feature -- in fact it's a more complex version.

Summary of changes: essentially the same as `B`, but we don't need to define/implement the `WorkerQueue` and `RustTask` interfaces.

#### Option D

I'm not completely sure how this one would work in practice, but I assume that it would mean TOML configuration for Kotlin/Swift similar to the current Desktop configuration:

```
[suggest.async_wrappers]
# All functions/methods are wrapped to be async by default and must be `await`ed.
enable = true
# These are exceptions to the async wrapping.  These functions must not be `await`ed.
main_thread = [
  "raw_suggestion_url_matches",
  "SuggestStore.new",
  "SuggestStore.interrupt",
  "SuggestStoreBuilder.new",
  "SuggestStoreBuilder.data_path",
  "SuggestStoreBuilder.load_extension",
  "SuggestStoreBuilder.remote_settings_bucket_name",
  "SuggestStoreBuilder.remote_settings_server",
  "SuggestStoreBuilder.build",
]
```

This would auto-generate async wrapper code.
For example, the Kotlin code would look something like this:

```
class SuggestStore {

    /**
     * Queries the database for suggestions.
     */
    suspend fun query(query: SuggestionQuery): List<Suggestion> =
        withContext(Dispatchers.IO) {
            // ...current auto-generated code here
        }
```

## Decision Outcome

## Pros and Cons of the Options

### (A) Keep the Rust code sync, but move it into app-services.

* Good, because it requires the least amount of work
* Good, because it's proven to work
* Good, because it clarifies the code ownership and makes it harder for us to forget about the functionality in the wrapper code.
* Good, because sync code can be easier to understand than async code.
* Bad, because abstracting async processes as sync ones can cause developers to miss details.
  For example, it's easy to forget about pending `query()` calls if `SuggestStore` methods are called synchronously in your mental model.
* Bad, because async will still be handled differently in JS vs Kotlin+Swift.
* Bad, because of the impact on documentation.

### (B) Async Rust using the current UniFFI

* Good, because we'll have a common system for Async that works similarly all platforms
* Good, because our generated docs will match how the methods are used in practice.
* Good, because it encourages us to move async complexity into the core code. 
  This makes it available to all platforms and more likely to be maintained.
* Good because it opens the door for more efficient thread usage.
  For example, we could make methods more fine-grained and only use the work queue for SQL operations, but not for network requests.
* Bad, because we're taking on risk by introducing the async UniFFI code to app-services.
* Bad, because our consumers need to define all a `WorkerQueue` implementations, which is a bit of a burden.
  This feels especially bad on JS, where the whole concept of a work queue feels alien.
* Bad, because it makes it harder to provide bindings on new languages that don't support async, like C and C++.
  Maybe we could bridge the gap with some sort of callback-based async system, but it's not clear how that would work.

### (C) Async Rust with additional UniFFI support

* Good/Bad/Risky for mostly the same reasons as (B).
* Good, because it removes the need for us and consumers to define `WorkerQueue` traits/impls.
* Good, because it can simplify the `WorkerQueue` code.
  In particular, we can guarantee that the task is only executed once, which removes the need for `RustTaskContainer`.
* Bad, because we'll have to maintain the UniFFI worker queue code.

### (D) Extend the uniffi-bindgen-gecko-js config system to Mobile

* Good, because we'll have a common system for Async that works similarly all platforms
* Mostly good, because our generated docs will match how the methods are used in practice.
  However, it could be weird to write docstrings for sync functions that are wrapped to be async.
* Good, because it's less Risky than B/C.
  The system would just be auto-generating the kind of wrappers we already used.
* Bad, because it's hard for consumer engineers to configure.
  For example, how can firefox-ios devs pick which QOS they want to use with a `DispatchQueue`?
  They would probably have to specify it in the config file, which is less convent than passing it in from code.
* Bad, because it's not clear how we could handle complex cases like using both a low-priority and high-priority queue.

## Decision Outcome
?
