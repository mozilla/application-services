# Embedded uniffi-bindgen for this workspace

This crate exists entirely to support generating the uniffi bindings.

Say what?

OK, so exposing a crate to Kotlin or Swift via `uniffi` is done in two halves:

* On the Rust side, the crate needs to depend on `uniffi` for runtime support code.
* On the foreign-language side, we need to also depend on `uniffi` to generate the language bindings.

It's very important that both halves use the exact same version of UniFFI, and UniFFI
will *try* to error out if it detects this. But if you *don't* detect it then using
mismatched versions of UniFFI will lead to an inscrutable linker failure at runtime.

We do this by using the builtin capabilities of uniffi - we add that crate as
a dev_dependency and run our handy
`cargo uniffi-bindgen` alias, which delegates to this crate.
XXX - update this?

You'll still have to ensure that all crates in this workspace are using the
same version of UniFFI, but that's much simpler than controlling what's installed
on everybody's build machine.
