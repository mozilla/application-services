# Using Async Rust

* Status: draft
* Deciders: 
* Date: ???

## Context and Problem Statement

Our Rust components are currently written using synchronous Rust.
The components are then wrapped in Kotlin to present an async interface.
Swift also wraps them to present an async-style interface, although it currently uses `DispatchQueue` and completion handlers rather than `async` functions.

UniFFI has been adding async capabilities in the last year and it seems possible to switch to using async Rust and not having a hand-written async wrapper.
It also seems possible to auto-generate the async wrapper with UniFFI.

What should our async strategy be?

### Scope

This ADR discusses what our general policy on wrapper code should be.
It does not cover how we should plan our work.
If we decide to embrace async Rust, we do not need to commit to any particular timeline for actually switching to it.

### Desktop and gecko-js

On desktop, we can't write async wrappers because it's not possible in Javascript.
Instead we use a strategy where every function is automatically wrapped as async in the C++ layer.
Using a config file, it's possible to opt-out of this strategy for particular functions/methods.

### Android-components

In Kotlin, the async wrapper layer currently lives in `android-components`.
For the purposes of this ADR, it doesn't really matter, and this ADR will not make a distinction between wrapper code in our repo and `android-components`.

## How it would work

### SQLite queries

One of the reasons our code currently blocks is to run SQLite queries.
https://github.com/mozilla/uniffi-rs/pull/1837 has a system to run blocking code inside an async function.
It would basically mean replacing code like this:

```kotlin
    override suspend fun wipeLocal() = withContext(coroutineContext) {
        conn.getStorage().wipeLocal()
    }
```

with code like this:
```rust

    async fn wipe_local() {
        self.queue.execute(|| self.db.wipe_local()).await
    }
```

We would need to merge #1837, which is currently planned for the end of 2023.

### Locks

Another reason our code blocks is to wait on a `Mutex` or `RWLock`.
There are a few ways we could handle this:

* The simplest is to continue using regular locks, inside a `execute()` call, which would be very similar to our current system.
* We could also consider switching to `async_lock` and reversing the order: lock first, then make a `execute()` call.
  This may be more efficient since the async task would suspend while waiting for the lock rather than blocking a thread
* We could also ditch locks and use [actors and channels](https://ryhl.io/blog/actors-with-tokio/) to protect our resources.
  It's probably not worth rewriting our current components to do this, but this approach might be useful for new components.

### Network requests

The last reason we block is for network requests.
To support that we would probably need some sort of "async viaduct" that would allow consumer applications to choose either:
- Use async functions from the `reqwest` library.
  This matches what we currently do for `firefox-ios`.
- Use the foreign language's network stack via an async callback interface.
  This matches what we currently do for `firefox-android`.
  This would require implemnenting https://github.com/mozilla/uniffi-rs/issues/1729, which is currently planed for the end of 2023.

## Decision Drivers

## Considered Options

* **(A) Experiment with async Rust**

* Pick a small component like `tabs` or `push` and use it to test our async Rust.
* Use async Rust for new components.
* Consider slowly switching existing components to use async Rust.

* **(B) Keep hand-written Async wrappers**

Don't change the status quo.

* **(C) Auto-generate Async wrappers**

We could also make the `gecko-js` model the official model and switch other languages to use it as well.
For example, we could support something like this in `uniffi.toml`:

```toml
[[bindings.async_wrappers]]
# Class to wrap, methods wrapped with an async version
wrapped = "LoginStore"
# Name of the wrapper class.
# UniFFI would generate async wrapper methods that worked exactly like the current hand-written code.
# For most languages, the wrapper class constructors would input an extra parameter to handle the async wrapping (for example `CoroutineContext` or `DispatchQueue`).
wrapper = "LoginStoreAsync"
# methods to skip wrapping and keep sync (optional)
sync_methods = [...]
```

We could also support async wrappers for callback interfaces.
These would allow the foreign code to implement an sync callback interface using async code
The Rust code would block while waiting for the result.

## Decision Outcome

## Pros and Cons of the Options

### (A) Experiment with async Rust

* Good, if we decide to avoid wrappers in `ADR-0008` because it allows us to remove the async wrappers.
* Bad, because there's a risk that the UniFFI async code will cause issues and our current async strategy is working okay.
  Even if we pick a small component to experiment with, it would be bad if that component crashes or stops responding because of async issues.
* Good because it allows us to be more efficient with our thread usage.
  When an async task is waiting on a lock or network request, it can suspend itself and release the thread for other async work.
  Currently, we need to block a thread while we are waiting for this.
  However, it's not clear this would meaningfully affect our consumers since we don't run that many blocking operations.
  We would be saving maybe 1-2 threads at certain points.
* Good, because it makes it easier to integrate with new languages that expect async.
  For example, WASM integration libraries usually returns `Future` objects to Rust which can only be evaluated in an async context.
  Note: this is a separate issue from UniFFI adding WASM support.
  If we switched our component code to using async Rust, it's possible that we could use `wasm-bindgen` instead.
* Bad, because it makes it harder to provide bindings on new languages that don't support async, like C and C++.
  Maybe we could bridge the gap with some sort of callback-based async system, but it's not clear how that would work.

### (B) Keep hand-written Async wrappers

* Good, this is the status quo, and doesn't require any work

### (C) Auto-generate Async wrappers

* Good, if we decide to avoid wrappers in `ADR-0008` because it allows us to remove the hand-written async wrappers.
* Good, because we could copy over auto-generated documentation and simply add `async` or `suspend` to the function signature.
* Good, because it's less risky than (A)
* Bad, because we would continue to have inefficiencies in our threading strategy.
* Good, because this is a more flexible async strategy.
  We could use async wrappers to integrate with languages like WASM and not use them to integrate with languages like C and C++.
  However, it's not clear at all how this would work in practice.
* Bad, because it's less flexible than (A).
  For example, with (A) it would be for the main API interface to return another interface with async methods from Rust code.
  That wouldn't be possible with this system, although it's not clear how big of an issue that would be.
