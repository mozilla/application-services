## Vendoring Application Services into mozilla-central

Some of these components are used in [mozilla-central](https://hg.mozilla.org/mozilla-central/).
This document describes how to update existing components or add new components.

The general process for [vendoring rust code into mozilla-central has its own
documentation](https://firefox-source-docs.mozilla.org/build/buildsystem/rust.html) -
please make sure you read that before continuing.

### Updating existing components.

To update components which are already in mozilla-central, follow these steps:

1. Ensure your mozilla-central build environment is setup correctly to make
   "non-artifact" builds - check you can get a full working build before
   starting this process.

1. The exact version of each component is specified [in the top-level Cargo.toml
  ](https://searchfox.org/mozilla-central/search?q=application-services+overrides&path=Cargo.toml) -
  just edit these lines.

1. From the root of the mozilla-central tree, execute `./mach vendor rust`.
   If this generates errors regarding duplicate crates, you will enter a world
   of pain, and probably need to ask for advice from the application-services
   team, and/or the [`#build` channel on matrix](https://matrix.to/#/#build:mozilla.org).

1. Verify that `git status` shows the crates you are trying to update have
   matching changes in the `third_party/rust` directory - that directory will
   should exactly match the application-services changes you are making. If
   that directory shows no changes, the `./mach vendor rust` command probably
   failed, so check its output.

1. Build and test your tree. Ideally make a try run.

1. Put your patch up to phabricator, requesting review from, at least, someone
   on the application-services team and one of the "build peers" - asking on
   [`#build` on matrix](https://matrix.to/#/#build:mozilla.org) for a suitable
   reviewer might help. Alternatively, try and find the bug which made the
   most recent update and ask the same reviewer in that patch.

1. Profit!

Notes:

* While not strictly required, it is best-practice to ensure all
  application-services components are on the same revision. This makes it
  easier to rationalize dependencies etc.

* The specified revision you vendor generally doesn't correspond to a specific
  release - we tend to not make releases just to update the vendored version.

* If the current `main` branch of application-services can't be taken due to
  breaking changes, but `mozilla-central` requires a new version for reasons
  internal to that repo (eg, maybe a tweak to dependencies, or because of
  the Rust version requirements etc), you will probably need to make a new
  branch on application-services and vendor from that - but even in that
  scenario, you specify the hash of the revision rather than the branch name.

### Adding a new component

A new component is slightly trickier, but not much - you will need to add the
new crate as a dependency to some `Cargo.toml` somewhere in the tree.
Exactly where you need to add this will depend on who is consuming the
component, but you will follow the same pattern as above:

* The consuming component will specify the dependency as a nominal "version 0.1"
* The top-level `Cargo.toml` will override that dependency with a specific git
  revision.

For example, consider the webext-storage crate:

* The [consuming crate specifies version 0.1
  ](https://searchfox.org/mozilla-central/search?q=MINIMUM_RUST_VERSION&path=python/mozboot/mozboot/util.py)
* The [top-level Cargo.toml](https://searchfox.org/mozilla-central/search?q=application-services+overrides&path=Cargo.toml)
  specifies the exact revision.

Adding a new component implies there will be related mozilla-central changes
which leverage it. The best practice here is to land both the vendoring of the
new component and the related `mozilla-central` changes in the same bug, but in
different phabricator patches. As noted above, the best-practice is that all
application-services components are on the same revision, so adding a new
component implies you will generally also be updating all the existing
components.

### Vendoring an unreleased version for testing purposes

Sometimes you will need to make changes in application-services and in mozilla-central
simultaneously - for example, you may need to add new features or capabilities
to a component, and matching changes in mozilla-central to use that new feature.

In that scenario, you don't want to check your changes in and re-vendor as you
iterate - it would be far better to use a local checkout of application-services
with uncommitted changes with your mozilla-central tree which also has uncommited
changes.

To do this, you can edit the top-level `Cargo.toml` to specify a path. Note
however that in this scenario, you need to specify the path to the
individual component rather than to the top-level of the repo.

For example, you might end up with something like:

```
# application-services overrides to make updating them all simpler.
interrupt-support = { path = "../application-services/components/support/interrupt" }
sql-support = { path = "../application-services/components/support/sql" }
sync15-traits = { path = "../application-services/components/support/sync15-traits" }
viaduct = { path = "../application-services/components/viaduct" }
webext-storage = { path = "../application-services/components/webext-storage" }
```

Note that when you first do this, it will still be necessary to run
`./mach vendor rust` and to re-build.

After you make a change to the local repository, you *do not* need to run
`./mach vendor rust`, but you do still obviously need to rebuild.

Once you are happy with all the changes, you would:
* Open a PR up in application-services and land your changes there.
* Follow the process above to re-vendor your new changes, and in that same
  bug (although not necessarily the same phabricator patch), include the other
  mozilla-central changes which rely on the new version.
