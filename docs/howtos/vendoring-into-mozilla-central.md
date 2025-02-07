## Vendoring Application Services into mozilla-central

Some of these components are used in [mozilla-central](https://hg.mozilla.org/mozilla-central/).
This document describes how to update existing components or add new components.

The general process for [vendoring rust code into mozilla-central has its own
documentation](https://firefox-source-docs.mozilla.org/build/buildsystem/rust.html) -
please make sure you read that before continuing.

### When to vendor

We want to keep our versions in moz-central relatively up-to-date, but it takes some manual effort
to do.  The main possibility of breakage is from a dependency mismatch, so our current vendoring
policy is:

  - Whenever a 3rd-party dependency is added or updated, the dev who made the change is responsible
    for vendoring.
  - At the start of the [release cycle](https://wiki.mozilla.org/Release_Management/Calendar) the
    triage owner is response for vendoring.

### Updating existing components.

To update components which are already in mozilla-central, follow these steps:

1. Ensure your mozilla-central build environment is setup correctly to make
   "non-artifact" builds - check you can get a full working build before
   starting this process.

1. Run `./tools/update-moz-central-vendoring.py [path-to-moz-central]` from the application-services
   root directory.

1. If this generates errors regarding duplicate crates, you will enter a world
   of pain, and probably need to ask for advice from the application-services
   team, and/or the [`#build` channel on matrix](https://matrix.to/#/#build:mozilla.org).

1. Run `./mach cargo vet` to check if there any any new dependencies that need to be vetted.  If
   there are ask for advice from the application-services team.

1. Build and test your tree. Ideally make a try run.

1. Put your patch up to phabricator, requesting review from, at least, someone
   on the application-services team and one of the "build peers" - asking on
   [`#build` on matrix](https://matrix.to/#/#build:mozilla.org) for a suitable
   reviewer might help. Alternatively, try and find the bug which made the
   most recent update and ask the same reviewer in that patch.

1. Profit!

### Adding a new component

Follow the [Uniffi documentation on mozilla-central](https://github.com/mozilla/gecko-dev/blob/master/docs/writing-rust-code/uniffi.md) to understand where you'll need to add your crate path and UDL. In general:

* The consuming component will specify the dependency as a nominal "version 0.1"
* The top-level `Cargo.toml` will override that dependency with a specific git
  revision.

For example, consider the webext-storage crate:

* The [consuming crate specifies version 0.1
  ](https://searchfox.org/mozilla-central/source/toolkit/components/extensions/storage/webext_storage_bridge/Cargo.toml#23)
* The [top-level Cargo.toml](https://searchfox.org/mozilla-central/search?q=application-services+overrides&path=Cargo.toml)
  specifies the exact revision.

Adding a new component implies there will be related mozilla-central changes
which leverage it. The best practice here is to land both the vendoring of the
new component and the related `mozilla-central` changes in the same bug, but in
different phabricator patches. As noted above, the best-practice is that all
application-services components are on the same revision, so adding a new
component implies you will generally also be updating all the existing
components.

For an example of a recently added component, [the tabs was recently added to mozilla-central with uniffi](https://bugzilla.mozilla.org/show_bug.cgi?id=1791851) and shows a general process to follow.

### Vendoring an unreleased version for testing purposes

Sometimes you will need to make changes in application-services and in mozilla-central
simultaneously - for example, you may need to add new features or capabilities
to a component, and matching changes in mozilla-central to use that new feature.

In that scenario, you don't want to check your changes in and re-vendor as you
iterate - it would be far better to use a local checkout of application-services
with uncommitted changes with your mozilla-central tree which also has uncommitted
changes.

To do this, you can edit the top-level `Cargo.toml` to specify a path. Note
however that in this scenario, you need to specify the path to the
individual component rather than to the top-level of the repo.

For example, you might end up with something like:

```
# application-services overrides to make updating them all simpler.
interrupt-support = { path = "../application-services/components/support/interrupt" }
relevancy = { path = "../application-services/components/relevancy" }
search = { path = "../application-services/components/search" }
suggest = { path = "../application-services/components/suggest" }
sql-support = { path = "../application-services/components/support/sql" }
sync15 = { path = "../application-services/components/sync15" }
tabs = { path = "../application-services/components/tabs" }
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
