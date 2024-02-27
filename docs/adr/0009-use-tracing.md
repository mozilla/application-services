# Replace logging and error reporting infrastructure with tracing.

* Status: proposed
* Deciders:
  * AppServices component teams
    * sync and related components: (decider name, date)
    * suggest, remote-settings, etc (ie, DISCO): (decider name, date)
    * (please add your team and decider name here!)
  * moz-central releng/build teams: ?
* Date: May 2025
* Feedback deadline: May 15 2025.

## Context and Problem Statement

Rust code in [application-services](https://github.com/mozilla/application-services/) needs support for diagnostics.
Specifically, we need support for logging and for explicit error reporting, as described below.

### Logging

Our components must be able to generate logging. These logs must be useful both when a component is being developed
(eg, sent to the console when running unit tests etc), but also useful when being run in a product
(eg, sent to the browser and captured normally for that browser - eg, the "console" on desktop, logcat on Android, etc.)

We currently use the `log` crate and wire this up best we can.

### Error Reporting

There's an [error support crate](https://github.com/mozilla/application-services/tree/main/components/support/error),
designed explicitly for error reporting for the applications. Unlike logins, these error reports are intended to
be reported to an external service - eg, Android and iOS both use Sentry; Desktop doesn't have this, but it should.

### Problems with the current approaches.

#### Problems using `log`

The main problem with the `log` module is the concept of a [global "max level"](https://docs.rs/cli-log/latest/cli_log/fn.set_max_level.html)
that's set for all crates.
Gecko sets the global max level to `Info` - anything more verbose causes [performance regressions in other crates](https://bugzilla.mozilla.org/show_bug.cgi?id=1874215).

In practice, this means there are cases where our browsers are unable to see debug logs from our Rust code.

#### Problems with the error reporter

None identified for mobile - but is not implemented at all for Desktop.

The opportunity is to better align the logging and error-reporting requirements into a single facility while introducing this capability
to desktop. In the first instance, we will arrange for Javascript code running in Desktop Firefox to be notified on such reports,
but a final decision about what it does with them is TBD.

## The Rust tracing crate.

An alternative to the `log` crate is the [`tracing`](https://docs.rs/tracing/latest/tracing/) crate,
which comes from the tokio project.

`tracing` has a very similar API to the `log` crate - `log::trace!` becomes `tracing::trace!` etc.
It has richer semantics than `log` (eg, async support) and largely acts a replacement -
with appropriate configuration it supports the `RUST_LOG` environment variable and writes to `stdout`,
so developers who are running tests and our CI etc should notice no difference.

Importantly, it has a [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)
mechanism that allows efficient, more targettted filtering instead of a global `max_level`.
Each subscriber is able to filter log events without impacting crates they aren't subscribed to.
This does mean it's impossible to have a global subscriber to see "all" logs - someone must explicitly subscribe by trace name.
However, it is precisely this characteristic which makes this option avoid causing performance issues in unrelated crates.

This means that it should be viable to have some crates capture `trace!()` output without impacting any other crates
or the overall performance of the application.

The merino project performed a [similar evaluation](https://github.com/mozilla-services/merino/blob/main/docs/adrs/adr_0001_logging.md)
and although with different constraints, also ended up choosing `tracing`.

## Proposal: Move to tracing for all app-services crates.

his document proposes that we move to a system based on `tracing`. We will do this by:

* Leveraging that all crates already depend on `error_reporting`, so have it export logging functionality based on `tracing`, including testing facilities.

* All app-services crates move to `error_reporting::debug!()` instead of `log::debug!()` etc.

* All exiting handling of log events be replaced with a new mechanism based on
  [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)
  to move logs and error-reports across the FFI.

* We use the same mechanism for error reporting, leveraging the richer metadata offered by tracing.

## Implementation: Move to tracing for all app-services crates.

This section describes the changes necessary to implement the above.

### Move all our crates to `tracing`

Most crates use `log::debug!()`/`log::error!()`; these all change to `use error_support::debug`/`info!("hi")` calls.

Testss starting with `env_logger::try_init().unwrap()` or similar will have an equivalent test helper.

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

This is generic enough to handle traditional "log" messages and our error reporting requirements. It's a general event reporting system.

[There's a WIP for all the above here](https://github.com/mozilla/application-services/compare/main...mhammond:application-services:log-to-tracing)

### Replace all existing "subscribers"

There are 3 main places which would change in the first instance, broken down by platform

#### Mobile

[A WIP for this is also included here](https://github.com/mozilla/application-services/compare/main...mhammond:application-services:log-to-tracing)

#### Desktop

Desktop has a [hand-written xpcom-based log adaptor](https://searchfox.org/mozilla-central/source/services/sync/golden_gate/src/log.rs#119-120).
This would be removed entirely and a uniffi-based callback mechanism used. Rust code calling back into Javascript has the same semantics as `golden_gate` - the log calls are "fire qnd forget", ending up in the main thread automatically.

* The [`gecko-logger`](https://searchfox.org/mozilla-central/source/xpcom/rust/gecko_logger/src/lib.rs) crate has all app-services code removed.
* New `gecko-tracing` crate next to `gwcko-logger` owns the single tracing-subscriber. app-services is the only `tracing_subscriber::Layer` provider, but more subscribers seems likely.
* [app-services-logger](https://searchfox.org/mozilla-central/source/services/common/app_services_logger/src/lib.rs) is removed.
* A JS "subscriber" will be implemented, a thin layer over the tracing crate and its `target` filtering.

#### Application Services

* All crates move to `tracing` instead of `log`

* A new crate would be added which defines the application callback interfaces (via UniFFI) and
  also the new tracing-subscriber implementation.

* The crates `rust-log-forwarder` and `error-reporter` crates would keep their external interface
  but would have their internal implementation replaced with the subscriber. This is for backwards
  compatibility with mobile - eventually we'd expose the new callback interfaces to mobile and delete
  these crates entirely.
