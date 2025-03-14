# Replace logging and error reporting infrastructure with tracing.

* Status: proposed
* Deciders:
  * AppServices team: ..
  * moz-central releng/build teams: ?
* Date: Mar 2025
* Feedback deadline: ?

## Context and Problem Statement

Rust code in [application-services](https://github.com/mozilla/application-services/) needs support for diagnostics.
Specifically, we need support for logging and for error reporting.

### Logging

Our components must be able to send logging to the browser - all platforms capture some logging.

We currently use the `log` crate and wire this up best we can.

### Error Reporting
There's an [error support crate](https://github.com/mozilla/application-services/tree/main/components/support/error),
designed explicitly for error reporting for the applications.
Android and iOS both hook into this to report errors to Sentry; Desktop doesn't have this, but it should.

### Problems with the current approaches.

#### Problems using `log`

The main problem with the `log` module is the concept of a [global "max level"](https://docs.rs/cli-log/latest/cli_log/fn.set_max_level.html)
that's set for all crates.
Gecko sets the global max level to `Info` - any more verbose causes [performance regressions in other crates](https://bugzilla.mozilla.org/show_bug.cgi?id=1874215).

In practice, this means our browsers are unable to see debug logs from our Rust code.

#### Problems with the error reporter

None for mobile - but is not implemented at all for Desktop.

The opportunity is to better align the logging and error-reporting requirements into a single facility while introducing this capability
to desktop.

## The Rust tracing crate.

An alternative to the `log` crate is the [`tracing`](https://docs.rs/tracing/latest/tracing/) crate,
which comes from the tokio project.

`tracing` has a very similar API to the `log` crate - `log::trace!` becomes `tracing::trace!` etc.
It has richer semantics than `log` (eg, async support) and largely acts a replacement -
it supports the `RUST_LOG` environment variable and writes to `stdout`,
so developers who are running tests and our CI etc should notice no difference.

Importantly, it has a [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)
mechanism that allows efficient, more targettted filtering instead of a global `max_level`.
Each subscriber is able to filter log events without impacting crates they aren't subscribed to.

This means that it should be viable to have some crates capture `trace!()` output without impacting any other crates
or the overall performance of the application.

## Proposal: Move to tracing for all app-services crates.

This document proposes that:

* All app-services crates move to using `tracing` instead of `log`.

* All exiting handling of log events be replaced with a new mechanism based on
  [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)
  to move logs and error-reports across the FFI.

* We use the same mechanism for error reporting, leveraging the richer metadata offered by tracing.

There are ways to make both `log` and `tracing` work, but we should avoid that if we can, just lean into tracing.

## Implementation: Move to tracing for all app-services crates.

This section describes the changes necessary to implement the above.

### Move all our crates to `tracing`

Most crates use `log::debug!()`/`log::error!()`, which changes to `tracing::...` (do we change them to just `debug!()`?)

Many tests start with `env_logger::try_init().unwrap()` or similar - we'll have a test helper.

### Implement a `tracing_subscriber::Layer`-based "subscriber" mechanism.

This subscriber mechanism requires the application to know all tracing `targets` it cares about.
Each crate is its own target and the app must explicitly "subscribe" to all targets individually.
It will *not* be possible to subscribe to all targets and it's unlikely we'll allow
any "matching" capabilities (eg, regular expressions or similar) - each target will use an exact string match.

This requires our applications to configure their own subscriptions to each `target` with the level for that target,
making it possible to avoid a single, global max-level.

We'll implement this subscriber with a simple `HashMap` mapping the target name to a level.
Once we have determined a subscriber matches an event, we can perform relatively expensive operations
on the event because we assume action will be taken on the event.
This assumption seems fine because we own all the subscribers.

An example of these "relatively expensive" operations is fetching event "fields", such as the message or other meta-data,
and using them to format a string, and dispatching the end result to the underlying logging
system.

Note that this is generic enough to handle traditional "log" messages and our error reporting requirements. It's a general event reporting system.

[There's a WIP for all the above here](https://github.com/mozilla/application-services/compare/main...mhammond:application-services:log-to-tracing)

### Replace all existing "subscribers"

There are 3 main places which would change in the first instance, broken down by platform

#### Mobile

[A WIP for this is also included here](https://github.com/mozilla/application-services/compare/main...mhammond:application-services:log-to-tracing)

#### Desktop

Desktop has a [hand-written xpcom-based log adaptor](https://searchfox.org/mozilla-central/source/services/sync/golden_gate/src/log.rs#119-120). This would be removed entirely and a uniffi-based callback mechanism is used. Rust code calling back into Javascript has the same semantics as `golden_gate` - the log calls are "fire and forget", ending up in the main thread automatically.

The [`gecko-logger`](https://searchfox.org/mozilla-central/source/xpcom/rust/gecko_logger/src/lib.rs) crate would change:
* All application-services log-related code would be removed entirely (eg, [here](https://searchfox.org/mozilla-central/source/services/interfaces/mozIAppServicesLogger.idl) and [here](https://searchfox.org/mozilla-central/source/services/common/app_services_logger)) -
app-services would not rely on `log` at all in this world.
* `gecko-logger` (or a similar crate next to it) would grow support for owning the single tracing-subscriber. It would be responsible for adding a single app-services owned `tracing_subscriber::Layer` instance to the single subscriber.

The [app-services-logger](https://searchfox.org/mozilla-central/source/services/common/app_services_logger/src/lib.rs) would lose all xpcom-related code and instead lean on uniffi and tracing-subscriber.

#### Application Services

* All crates move to `tracing` instead of `log`

* A new crate would be added which defines the application callback interfaces (via UniFFI) and
  also the new tracing-subscriber implementation.

* The crates `rust-log-forwarder` and `error-reporter` crates would keep their external interface
  but would have their internal implementation replaced with the subscriber. This is for backwards
  compatibility with mobile - eventually we'd expose the new callback interfaces to mobile and delete
  these crates entirely.
