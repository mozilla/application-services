# Wrapper code

* Status: draft
* Deciders: 
* Date: ???

## Context and Problem Statement

Application-services components currently consist of a Rust core, and UniFFI-generated bindings for Swift and Kotlin.
Additionally, some of our components have hand-written Swift and Kotlin code, containing wrappers and extensions for the UniFFI bindings.
In the past, these wrappers were strictly necessary because of deficiencies in our FFI strategy.
However, UniFFI is reaching the point where it can support all our requirements and it's possible to remove the wrapper code.

We should decide how much effort, if any, we want to invest into getting rid of the wrapper code now that it's technically possible.
We should also decide if there are places where wrapper code makes sense and we don't want to replace it, even ignoring the time investment.

### Possible changes

This section explores some possibilities for removing wrapper code.
Deciding on any particular possibility listed here is out of scope for the ADR, the decision is if we should invest our time investigating some of these.

#### Async

One of the main reasons for our current wrapper code is to wrap our sync API and present an async interface.
ADR-0009 lays out a couple of alternatives to this.

#### Feature flags for breaking changes

Another main reason for our current wrapper code is to mitigate breaking API changes.
The wrapper layer allows us to make breaking changes at the Rust level, but keep the same API at the wrapper layer.

An alternative to this would be to use Rust feature flags to manage breaking changes.
Any breaking change, or large change in general, would be behind a feature flag.
We would wait to enable the feature flag on the megazord until consumer application was ready to take the change.
Maybe we could have a transition period where we built two megazords, for example `megazord` and `megazord-with-logins-local-encrytion` and the consumer app could pick between the two.
This would simplify the consumer PRs since they could run CI normally.

Some potential uses of of features flags would be:

- **Cosmetic changes**. If we want to rename a field/function name, we could put that rename behind a feature flag.
- **Architectural changes**. For bigger changes, like the logins local encryption rework, we could maintain two pieces of code at once. For example, copy db.rs to db/old.rs and db/new.rs. Create db/mod.rs which runs `use old::*` or `use new::*` depending on the feature flag. Then we do our work in db/new.rs.

Maintaining many feature flags at once would require significant effort, so we should aim to get our consumers to use the new feature flag soon after our work is complete.

#### UniFFI-supported interfaces

One last reason for wrapper code is to present idiomatic interfaces to our Rust code -- especially for callback interfaces.
For example, it's possible to define a UniFFI callback interface for notifications, but the FxA wrapper code uses the `NotificationCenter` on Swift which is not supported by UniFFI.
If we wanted to remove all wrapper code we would need to commit to only using interfaces that UniFFI could support.

## Decision Drivers

## Considered Options

* **(A) Keep using our current strategy**
Don't invest time into removing wrapper code.

* **(B) Remove all wrapper code**
Invest time into removing wrapper code with the intention of removing all of it.

* **(C) Remove most wrapper code, keep an additive wrapper layer**
Invest time into removing wrapper code, but keep some wrapper code.
In particular, keep a wrapper layer that adds new API surfaces rather than replaces existing ones.
For example, we could define an `FxaEventListener` interface with UniFFI, then add a `IosEventListener` class that implemented that interface by forwarding the messages to the `NotificationCenter`.

## Decision Outcome

## Pros and Cons of the Options

### (A) Keep using our current strategy

* Good, because we can spend our time on other improvements
* Good, because there's no chance of wasting time on implementing solutions that may not work out in practice.

### (B) Remove all wrapper code
* Good, because it simplifies our documentation strategy.
  There is active work in UniFFI to auto-generate the bindings documentation from the Rust docstrings (https://github.com/mozilla/uniffi-rs/pull/1498, https://github.com/mozilla/uniffi-rs/pull/1493).
  If there are no wrappers, then we could potentially use this to auto-generate a high-level documentation site and/or docstrings in the generated bindings code.
  If there are wrappers, then this probably isn't going to work.
  In general, wrappers mean we are have multiple public APIs which makes documentation harder.
* Bad, because it may lead to worse documentation.
  Hand-written documents can be better than auto-generated ones and especially since they can specifically target javadoc and swiftdoc.
* Good, because a "vanilla" API may be easier to integrate with multiple consumer apps.
  `NotificationCenter` might be the preferred choice for firefox-ios, but another Swift app may want to use a different system.
  By only using UniFFI-supported types we can be fairly sure that our code will work with any system.

### (C) Remove most wrapper code, keep an additive wrapper layer
* Good, because most documentation can be auto-generated and some can be hand-written.
  The hand-written documentation would be the language-specific parts, which probably need to be written by hand.
* Bad because it may lead to worse documentation, for the same reasons an (B).
* Good, because consumer apps can choose to use the wrapper interfaces or not.
