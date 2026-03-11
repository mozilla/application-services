This directory holds a couple of crates which are hacks to help us on the
path to moving this repo into mozilla-central.

They exist only to keep Cargo and/or the Rust compiler happy, so the
same code/config can be used in both repos. They serve no actual purpose
and are never actually called in this repo.

They are:

* workspace-hack/* contains a dummy `mozilla-central-workspace-hack` crate, which
  is referenced from a few "top-level" crates built in m-c.

* mozbuild/* contains a dummy `mozbuild` crate, which is referenced by the ohttp
  crate when the "app-svc" feature is enabled. That feature auto-detects whether
  it is in this repo or in mozilla-central.
