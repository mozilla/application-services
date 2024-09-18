# Embedded uniffi-bindgen for this workspace

This crate exists entirely to support running `cargo uniffi-bindgen` in the workspace
root and having it correctly execute the `uniffi-bindgen` command provided by the
version of `uniffi_bindgen` that's in use in the workspace.

Say what?

OK, so exposing a crate to Kotlin or Swift via `uniffi` is done in two halves:

* On the Rust side, the crate needs to depend on `uniffi` for runtime support code.
* On the foreign-language side, we need to run `uniffi-bindgen` as part of the build
  process to generate the language bindings.

It's very important that both halves use the exact same version of UniFFI, and UniFFI
will *try* to error out if it detects this. But if you *don't* detect it then using
mismatched versions of UniFFI will lead to an inscrutable linker failure at runtime.

The simple way to use UniFFI is for developers to `cargo install uniffi_bindgen`
on their system and then use the `uniffi-bindgen` command-line tool during the build.
But unfortunately, this makes it painful to bump the version of the `uniffi` crate
used by the components, and it greatly complicates doing local development on UniFFI
itself.

This crate offers a more complicated way that is also more robust to UniFFI version
changes.

First, configure all UniFFI-using crates to use the `builtin-bindgen` feature
of `uniffi_build`.

Then, instead of running the system-installed `uniffi-bindgen`, run our handy
`cargo uniffi-bindgen` alias, which delegates to this crate. This will execute
the `uniffi-bindgen` command-line tool *provided by the version of UniFFI specified
in this crate*, letting us avoid having to install and update `uniffi-bindgen` at
the system level

You'll still have to ensure that all crates in this workspace are using the
same version of UniFFI, but that's much simpler than controlling what's installed
on everybody's build machine.
