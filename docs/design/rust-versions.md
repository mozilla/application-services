# Rust Versions

Like almost all Rust projects, the entire point of the application-services
components is that they be used by external projects. If these components
use Rust features available in only the very latest Rust version, this will
cause problems for projects which aren't always able to be on that latest
version.

Given application-services is currently developed and maintained by Mozilla
staff, it should be no surprise that an important consideration is
mozilla-central (aka, the main Firefox repository).

## Mozilla-central Rust policies.

It should also come as no surprise that the Rust policy for mozilla-central
is somewhat flexible. There is an official [Rust Update Policy Document
](https://firefox-source-docs.mozilla.org/writing-rust-code/update-policy.html)
but everything in the future is documented as "estimated".

Ultimately though, that page defines 2 Rust versions - "Uses" and "Requires",
and our policy revolves around these.

To discover the current, actual "Uses" version, there is a [Meta bug on Bugzilla](https://bugzilla.mozilla.org/show_bug.cgi?id=1504858) that keeps
track of the latest versions as they are upgraded.

To discover the current, actual "Requires" version, [see searchfox](https://searchfox.org/mozilla-central/search?q=MINIMUM_RUST_VERSION&path=python/mozboot/mozboot/util.py)

# application-services Rust version policy

Our official Rust version policy is:

* All components will ship using, have all tests passing, and have clippy emit
  no warnings, with the same version mozilla-central currently "uses".

* All components  must be capable of building (although not necessarily with
  all tests passing nor without clippy errors or other warnings) with the same
  version mozilla-central currently "requires".

* This policy only applies to the "major" and "minor" versions - a different
  patch level is still considered compliant with this policy.

## Implications of this

All CI for this project will try and pin itself to this same version. At
time of writing, this means that [our circle CI integration
](https://github.com/mozilla/application-services/blob/main/.circleci/config.yml) and
[rust-toolchain configuration](https://github.com/mozilla/application-services/blob/main/rust-toolchain.toml)
will specify the versions (and where possible, the CI configuration file will
avoid duplicating the information in `rust-toolchain`)

We should maintain CI to ensure we still build with the "Requires" version.

As versions inside mozilla-central change, we will bump these versions
accordingly. While newer versions of Rust can be expected to work correctly
with our existing code, it's likely that clippy will complain in various ways
with the new version. Thus, a PR to bump the minimum version is likely to also
require a PR to make changes which keep clippy happy.

In the interests of avoiding redundant information which will inevitably
become stale, the circleci and rust-toolchain configuration links above
should be considered the canonical source of truth for the currently supported
official Rust version.
