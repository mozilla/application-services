# Replace logging and error reporting infrastructure with tracing.

* Status: proposed
* Deciders:
  * AppServices team: ?
  * Firefox Android: ?
  * Firefox iOS: ?
  * Firefox Desktop: ?

* Date: Mar 06, 2024
* Feedback deadline: Mar 20, 2024

## Context and Problem Statement

Currently all Rust code in [app-services](https://github.com/mozilla/application-services/)
uses the `log` crate for debugging and diagnostics.
Log output is valuable for working on the code, and for helping users troubleshoot.
When working on the code, log output is often captured during tests, where it's just printed to stdout/stderr if the test fails.
For helping users troubleshoot, log output is sent across the FFI so it can be captured in whatever "native" logging system is in place.
Users or QA can then export and attach the logs to a bug.

application-services also has an [error support crate](https://github.com/mozilla/application-services/tree/main/components/support/error),
designed explicitly for error reporting for the applications.
Android and iOS both hook into this to report errors to Sentry, whereas Desktop has not yet implemented this functionality.
While the error support crate has capabilities beyond just moving the error across the FFI, this capability is the focus of this document,
and any changes proposed to this crate by this document are limited to just this.

### Problems with the current approaches.

#### Problems using `log`

The main problem with the `log` module is a design choice made by that crate,
which is the concept of a [global "max level"](https://docs.rs/cli-log/latest/cli_log/fn.set_max_level.html)
that's set for all crates.

The Gecko code has embraced this design choice and sets the global max level to Info for production builds.
Changing this max level to, say, Trace can cause performance regressions in other crates - eg, see this desktop bug for one example.
Changing all the existing code is a difficult, and probably unwelcome, task, and is only possible for crates we control.

In practice, this means that we are unable to selectively get `Debug` or `Trace` logs for individual components,
because the max level for the logger is, effectively, fixed at `Info`.

#### Problems with the error reporter

The error reporter actually works OK for mobile, but is not implemented at all for Desktop.
However, the existing implementation is quite ad-hoc and not very "rich" in terms of data which
can be sent to the error reporter.

So while there's actually no real problem here, there is an opportunity to better align the
logging and error-reporting requirements into a single facility before introducing this capability
to desktop.

## The Rust tracing crate.

An alternative to the `log` crate is the [`tracing`](https://docs.rs/tracing/latest/tracing/) crate,
which comes from the tokio project.
This document is not going to rehash the excellent existing documentation, but some key features
include:

* For simple logging it has a very similar API to the `log` crate. Almost all existing use of `log` in non
  test code can be upgrade to tracing simply by replacing `log::trace!` with `tracing::trace!` etc.
* Richer logging semantics mean that it's quite easy and efficient to attach additional data to each log entry.
  ie, more than simple strings can be logged.
* A [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/) mechanism that allows
  efficient filtering etc of log entries without enforcing a global `max_level`.
  Each subscriber is able to efficiently filter log events without impacting other crates.

This means that it should be viable to have some crates capture `Trace` output without impacting any other crates
or the overall performance of the application..

## Proposal: Move to tracing for all app-services crates.

This document proposes that:

* All app-services crates move to using `tracing` instead of `log`.
  There is a `tracing_log` crate which should mean we don't need to move all crates, but using this
  crate does have some limitations, and given the move to `tracing` is largely mechanical we should probably
  avoid it if we can - but it's a reasonable fallback if we need log output from crates we don't control.

* All techniques we have for reporting log events and error reports to the application should be replaced with
  a new mechanism based on [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)
  to move logs and error-reports across the FFI.

* This same mechanism, and the same raw data, should be used on all platforms (ie, Android, iOS and Desktop).

To be explicit, there would be no requirement for any other crates to move to tracing. However, we also
believe there is nothing in this document which would prevent any crates from using tracing.
Further, there's no reason that the use of `log` and `tracing` is mutually exclusive; crates are, and will remain, free
to use either of these crates as they see fit.

## Implementation: Move to tracing for all app-services crates.

This section describes the changes necessary to implement the above.

### Move all our crates to `tracing`

Most crates tend to use `log::debug!()` (or `trace!`, `error!` etc). We'd just change the crate to depend on `tracing` instead of `log`,
and probably change each module to have as a prelude `use tracing::{trace, debug, info, error};` and change each `log::error!` to `error!`.
Or maybe we just spell it out entirely as `tracing::log!(...)` - this is a choice for individual crate maintainers.

Many tests start with `env_logger::try_init().unwrap()` or similar - these would all be replaced with the more-verbose but equivalent

```
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::from_default_env())
            .init();
```

(probably via a helper of some sort)

### Implement a `tracing_subscriber::Layer`-based "subscriber" mechanism.

This subscriber mechanism would require the application to know all tracing `targets` it cares about - and by default,
each crate is its own target. The app must explicitly "subscribe" to all such targets individually.
It will *not* be possible to subscribe to all events for performance reasons, and it's unlikely we'll allow
any "matching" capabilities (eg, regular expressions or similar) - each target will use an exact string match.

When subscribing to a target, the app will also need to specify the max-level for that target. In other words, a
subscription can be considered a tuple of `(target, level)`.

This implies that checking whether an individual tracing event has a subscriber will be a `HashMap` lookup for the target name,
then a numeric comparison for the level. Each target will allow only one subscriber, identified by the `target`.
This means that changing the level for an existing subscriber will involve replacing the subscription with a new one which holds the new level.
It is expected that the implementation will simply be "set a new subscription", which will replace the old subscription.

This assumes that the application will subscribe with the actual level needed by that subscriber - ie, that apps will not
simply subscribe to the highest log level and perform additional filtering after the log event has been dispatched.
This means that the "performance critical path" for this process is determining if we have a subscriber;
once we have determined a subscriber matches an event, we can perform relatively expensive operations
on the event because we assume action will be taken on the event.
We believe this assumption is fine because we will own all such subscribers.

An example of these "relatively expensive" operations is fetching event "fields", such as the message or other meta-data.

You will note that this is generic enough to handle traditional "log" messages, our error-reporting logs, and more structured logging.
The shape of the structs etc (eg, try and use one struct for all use-cases, one struct per use-case, etc) is intentionally omitted to avoid bike-shedding on those details in the first instance.

### Replace all existing "subscribers"

There are 3 main places which would change in the first instance, broken down by platform

#### Mobile

As described in the Application Services section, the existing crates used by mobile would keep the same public API
in the first instance, meaning our mobile apps would not require changes; however, eventually we'd look at moving
the mobile apps to the new mechanism, at which time we'd kill the 2 "legacy" app-services crates.

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
