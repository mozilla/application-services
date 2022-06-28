<!-- This ADR is using a lighter weight template created by Michael Nygard (see [here](https://github.com/joelparkerhenderson/architecture-decision-record/blob/main/templates/decision-record-template-by-michael-nygard/index.md) for details). It is -->

# Remove Unnecessary Swift `DispatchQueue` Usage

## Status

* Proposed

## Context

### `DispatchQueue` Overview

A `DispatchQueue` is a Swift abstraction used to manage the serial or concurrent execution of tasks. When a `DispatchQueue` is created, a thread pool (comprised of one or more reallocated or newly-created threads) is assigned to the queue by the system. `DispatchQueue`s can be configured to execute tasks, or "work items", serially by default or concurrently if the [attributes](https://developer.apple.com/documentation/dispatch/dispatchqueue/attributes) parameter of the constructor is appropriately set.

Other [constructor parameters](https://developer.apple.com/documentation/dispatch/dispatchqueue/2300059-init) allow for additional configurations but they are not relevant here as the custom queues created in our rust components and in the corresponding [firefox-ios storage layer](https://github.com/mozilla-mobile/firefox-ios/tree/main/Storage/Rust) set the `label` parameter of the `DispatchQueue` constructor at most and therefore execute serially with default settings.

We also make use of the two non-custom `DispatchQueue`s, the [main](https://developer.apple.com/documentation/dispatch/dispatchqueue/1781006-main) and [global](https://developer.apple.com/documentation/dispatch/dispatchqueue/2300077-global) queues, in the FxA component. Unsurprisingly, the main queue executes on the main thread. The global queues (high, default, low, and background queues) execute concurrently and are shared by the whole system. Generally, best practice dictates that the main queue be reserved for UI/UX updates and global and custom queues (which are ultimately performed by global queues) do the remainder of the work.

Lastly, tasks can be submitted to a `DispatchQueue` either synchronously (which blocks the queue until the submitted task is complete) or asynchronously (which returns immediately after the task is submitted to the queue). Best practice advises that synchronous calls be used only when needed and should be avoided entirely when scheduleding tasks concurrently or on the main queue.

### Our `DispatchQueue` Usage

There are places in application services code where `DispatchQueues` are appropriately used. In the FxA component where we have functions that update the UI (mostly via [closure function parameters](https://github.com/mozilla/application-services/blob/451bcc2fe8fe6675ca3de962a4b38b3fa181a806/components/fxa-client/ios/FxAClient/FxAccountManager.swift#L108)) or [dispatch notifications via `NotificationCenter`](https://github.com/mozilla/application-services/blob/72b827c3e0f883163762857fd766df1aeb060725/components/fxa-client/ios/FxAClient/FxAccountDeviceConstellation.swift#L49), our use of non-custom queues makes sense.

The same is true of the custom `DispatchQueue` usage in the RustLog crate where we want to ensure that the `state` property of the singleton is accurate. However our usage of `DispatchQueue`s in the places, logins, and tabs rust components is unnecessary and potentially problematic. In these components we create a custom `DispatchQueue` for our API functions and submit tasks to the queue for synchronous execution similar to the snippet below.

```
// The swift layer in appServices

private let queue = DispatchQueue(label: "com.mozilla.logins-storage")
...
open func reset() throws {
    try queue.sync {
        try self.store.reset()
    }
}
```
Then in the Firefox iOS where these functions are called, they are submitted for asynchronous execution to another custom `DispatchQueue` created specifically for the storage layer of the respective component as shown below.

```
// The storage layer in Firefox iOS

queue = DispatchQueue(label: "RustLogins queue: \(databasePath)", attributes: [])
...
public func resetSync() -> Success {
    let deferred = Success()

    queue.async {
        ...
        try self.storage?.reset()
        ...
    }
    ...
}
```

In practice this means there are two queues with fairly similar call stacks. At minimum this is unintentional duplication that has minimal adverse impact. But depending on how the system allocates threads for the `DispatchQueue`s in question we may be causing unnecessary thread creation or forcing tasks that should be executing aynchronously to execute synchronously (as in the above example).

## Decision

We will be removing `DispatchQueue` usage in application services from the places, logins, and tabs components. We will start with the logins component because it is currently stable. If that removal is successful, we will remove our queue usage from the other two components.

## Consequences

* This will give the iOS team and any future consumers of the places, logins, and tabs components more control over how system resources are managed for their respective applications.
* The application services swift layer will become a bit easier to reason about.

## Resources

* [Swift DispatchQueue Documentation](https://developer.apple.com/documentation/dispatch/dispatchqueue)
* [Understanding Queue Types](https://www.raywenderlich.com/28540615-grand-central-dispatch-tutorial-for-swift-5-part-1-2#toc-anchor-005)
* [Appropriately using DispatchQueue.main](https://www.donnywals.com/appropriately-using-dispatchqueue-main/)
* [Concurrent vs Serial DispatchQueue: Concurrency in Swift explained](https://www.avanderlee.com/swift/concurrent-serial-dispatchqueue/)
