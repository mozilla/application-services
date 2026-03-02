## Vendoring Application Services into mozilla-central

Some of these components are used in [mozilla-central](https://hg.mozilla.org/mozilla-central/).
This document describes how to update existing components or add new components.

NOTE: This process updates the components which are built into libxul for Desktop Firefox builds.
This does not change what code is built for Android or iOS.

### When to vendor

We want to keep our versions in moz-central relatively up-to-date, but it takes some manual effort
to do.  The main possibility of breakage is from a dependency mismatch, so our current vendoring
policy is:

  - Whenever a 3rd-party dependency is added or updated, the dev who made the change is responsible
    for vendoring.
  - At the start of the [release cycle](https://wiki.mozilla.org/Release_Management/Calendar) the
    triage owner is response for vendoring.

We are aiming to move to a more regular process, done automatically such that the version in
m-c is the same as shipped to Android via our Maven publications. It remains to be seen whether
we can actually achieve this, but the aim is to make this more regular and as automated as possible.

### Updating existing components.

This process uses the standard mozilla-central facilities for [vendoring third party components](https://firefox-source-docs.mozilla.org/mozbuild/vendor/index.html)

To update components which are already in mozilla-central, follow these steps:

1. Ensure your mozilla-central build environment is setup correctly to make
   "non-artifact" builds - check you can get a full working build before
   starting this process. All commands listed below should be run from the root
   if your mozilla-central checkout.

1. Run `./mach vendor third_party/application-services/moz.yaml --force -r app-services-commit-hash`,
   where `app-services-commit-hash` should be replaced with the actual hash of the commit you want
   to vendor, which is probably whatever `main` is on.

1. Run `./mach vendor rust` to update any dependencies which may have changed since the last
   vendor. In most cases this should do nothing. In sad cases this might require you to vet the
   new dependencies and get reviews from the supply chain reviewers group.

1. Run `./mach uniffi generate`, which may cuase changes to be recorded for the JS bindings if
   any of the vendored crates have had changes to their FFI.

1. Build and test your tree. Ideally make a try run.

1. Put your patch up to phabricator, requesting review from, at least, someone
   on the application-services team and one of the "build peers" - asking on
   [`#build` on matrix](https://matrix.to/#/#build:mozilla.org) for a suitable
   reviewer might help. Alternatively, try and find the bug which made the
   most recent update and ask the same reviewer in that patch.

1. Profit!

### Adding a new component

Follow the [Uniffi documentation on mozilla-central](https://github.com/mozilla/gecko-dev/blob/master/docs/writing-rust-code/uniffi.md) to understand where you'll need to add your crate path and UDL.

* You will need to add a reference to the crate in the top-level Cargo.toml, near the [other app-services references](https://searchfox.org/firefox-main/search?q=%23%20application-services%20overrides).

* You will need to arrange for the UniFFI JS bindings to create a wrapper for your component.
[Follow the instructions in mozilla-central for this](https://firefox-source-docs.mozilla.org/rust-components/developing-rust-components/uniffi.html)
