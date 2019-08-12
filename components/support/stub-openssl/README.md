# stub-openssl

This is a "stub" version of the Rust [openssl crate](https://crates.io/crates/openssl)
that exposes compilable-but-not-runnable a subset of its API. It can be used to avoid
actually linking in OpenSSL when compiling crates that have a hard dependency on it
at build time, but can be configured not to actually use it at runtime.

To use it, configure your `Cargo.toml` to replace the `openssl` crate with this stub,
like so:

```
[patch.crates-io]
openssl = { path = "components/support/stub-openssl" }
```

You might need to stub out some more of the `openssl` API to get things to compile,
but once things do compile, it should result in a functioning library that will
panic if anything tries to use OpenSSL APIs at runtime.

XXX TODO: figure out the right copyright declaration etc for this API-compatible code.
