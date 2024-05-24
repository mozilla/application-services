# Sync Manager

The sync manager designed to integrate multiple sync engines/stores into
a single coherent framework. There are multiple, independent rust components,
all of which know how to sync - but when an app uses multiple components,
something must tie them together so that, eg, a single "sync now" function
exists to sync all components, and to help the app manage the "enabled" state
of these engines. There's other non-obvious "global state" which should also
be shared - eg, we only need a single token from the token-server and can share
it across all components, if the server is under load and wants clients to
back off, that state should be shared.

This crate relies very heavily on `sync15`, so you should read the
documentation in that crate. Indeed, you can almost see this as a wrapper
around that crate, but there's other "global functionality" managed by this
crate that doesn't fit well in any other place. For example, each application
should have a single "device record" which describes the app and not individual
stores. There's also the concept of "commands" which are sent to a device, and
then delegated to the correct store - those concepts are implemented in this
crate.

## Other notes:

It's a bit unfortunate this component can't just be part of `sync15`.
Conceptually and functionally, it shares most in common with with the `sync15`
crate, and in some cases stretches (or arguably breaks) the abstraction barrier
that `sync15` puts up.

Unfortunately, places/logins/etc depend on sync15 directly, and so to prevent a
dependency cycle (which is disallowed by cargo), doing so would require
implementing the manager in a fully generic manner, with no specific handling of
underlying crates. This seems extremely difficult, so this is split out
into it's own crate, which might happen to reach into the guts of `sync15` in
some cases.

Note also that the world has changed a little since then - in particular, we
now have `sync15-traits` - there might now be an opportunity to move all the
types into that crate, and merge `sync15` and `sync_manager`
