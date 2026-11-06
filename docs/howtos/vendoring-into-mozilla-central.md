## Vendoring Application Services into mozilla-central

Some of these components are used in [mozilla-central](https://hg.mozilla.org/mozilla-central/).
This document describes how to update existing components or add new components.

NOTE: This process updates the components which are built into libxul for Desktop Firefox builds.
This does not change what code is built for Android or iOS.

### When to vendor

Changes on `main` reach Desktop Firefox only when a new revision is vendored, which does not happen
on a regular candace. Vendor when you're motivated to get your changes released. Changes that add 
or update a 3rd-party dependency should be vendored promptly, as dependency mismatches are the most
likely source of breakage.

Vendored code follows the mozilla-central release trains. To ship in a given Firefox version, the
vendor patch must land before that version's merge to beta; see the
[release calendar](https://whattrainisitnow.com/calendar/).

We are aiming to move to a more regular process, done automatically such that the version in
m-c is the same as shipped to Android via our Maven publications. It remains to be seen whether
we can actually achieve this, but the aim is to make this more regular and as automated as possible.

### Prerequisites

- A mozilla-central checkout
- [Commit access](https://www.mozilla.org/en-US/about/governance/policies/commit/access-policy/):
  Level 1 to push to try, Level 3 to land.
  Without the required level, prepare and submit the patch as below and ask the reviewer to push
  to try and land it.

### Updating existing components.

This process uses the standard mozilla-central facilities for [vendoring third party components](https://firefox-source-docs.mozilla.org/mozbuild/vendor/index.html)

1. File a bug in `Application Services :: General` titled `Vendor application-services <short-hash>` ([template](https://bugzilla.mozilla.org/enter_bug.cgi?product=Application%20Services&component=General&bug_type=task&short_desc=Vendor%20application-services%20%3Cshort-hash%3E)).
   Summarize the changes being vendored in the description. The currently vendored revision is
   recorded in `third_party/application-services/moz.yaml`.

2. Run `./mach vendor third_party/application-services/moz.yaml -r <commit-hash>`, where
   `<commit-hash>` is the application-services commit to vendor, normally the tip of `main`.

3. Run `./mach vendor rust --ignore-modified` to update `Cargo.lock` and vendored crates.
   `--ignore-modified` is required because the previous step leaves uncommitted changes.
    - This step is required whenever an application-services crate's dependency list has changed
      since the last vendor, including internal and dev dependencies. mozilla-central builds with
      `--frozen`, so a stale `Cargo.lock` fails the build.
    - If new 3rd-party dependencies were added, they must be vetted and reviewed by the supply
      chain reviewers group.

4. Run `./mach uniffi generate` to regenerate the JS bindings

5. Build and test, then push to try (e.g. `./mach try auto`) and link the try push from the bug.

6. Commit with a message like `Bug NNNN - Vendor application-services <short-hash>. r?reviewer`
   and submit with `moz-phab submit`. Request review from someone on the application-services
   team. If 3rd-party dependencies changed, also request one of the "build peers"; ask on
   [`#build` on matrix](https://matrix.to/#/#build:mozilla.org) for a suitable reviewer, or use the
   reviewer from the most recent vendor bug.

7. After approval and a green try run, land with Lando.

### Adding a new component

Follow the [Uniffi documentation on mozilla-central](https://github.com/mozilla/gecko-dev/blob/master/docs/writing-rust-code/uniffi.md) to understand where you'll need to add your crate path and UDL.

* You will need to add a reference to the crate in the top-level Cargo.toml, near the [other app-services references](https://searchfox.org/firefox-main/search?q=%23%20application-services%20overrides).

* You will need to arrange for the UniFFI JS bindings to create a wrapper for your component.
[Follow the instructions in mozilla-central for this](https://firefox-source-docs.mozilla.org/rust-components/developing-rust-components/uniffi.html)
