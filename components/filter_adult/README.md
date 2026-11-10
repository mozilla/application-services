# Filter Adult

The Filter Adult module will compare the base domain string of a URL against a
static list of base domains for what are considered "adult sites".

A "base domain" in this context is the same base domain that is returned by
[nsIEffectiveTLDService.getBaseDomain](https://searchfox.org/mozilla-central/rev/85d6bf1b521040c79ed72f3966274a25a2f987c7/netwerk/dns/nsIEffectiveTLDService.idl#54-94).

## Updating the list

`src/adult_set.bin` contains the bytes for the hash set of adult sites.

First, map the list of hostnames to base domains with
`scripts/map-base-domains.js`. It runs in `xpcshell` from a Firefox build, so
the base domains match what Firefox computes. See the script for usage.

Then regenerate `adult_set.bin` from the resulting file:

```shell
cargo run -p filter_adult --features build-binary --bin import-site-list -- path/to/domains.txt components/filter_adult/src/adult_set.bin
```

The domain list itself is not checked in.

## Tests

Tests are run with

```shell
cargo test -p filter_adult --features build-binary
```

## Bugs

We use Bugzilla to track bugs and feature work. You can use [this link](bugzilla.mozilla.org/enter_bug.cgi?product=Firefox&component=New Tab Page) to file bugs in the `Firefox :: New Tab Page` bug component.
