# url-macro

Compile-time URL string validation for application-services.

## What it is

A proc macro that validates a URL string literal at compile time. Invalid
input becomes a compile error pointing at the offending literal; valid
input expands to a runtime `url::Url::parse` call that is guaranteed to
succeed.

```rust
use url::Url;
use url_macro::url;

let endpoint: Url = url!("https://ads.mozilla.org/v1/");
// url!("not a url");          // → compile error: relative URL without a base
// url!();                     // → compile error: expected string literal
```

## Why we built it

Today the monorepo carries **205 occurrences** of `Url::parse(...).expect/unwrap`
across **30 files**, in **three incompatible patterns** (`Lazy<Url>` +
`.expect()`, `const &str`, inline `Url::parse()` in `match` arms). At
least one literal — `https://ads.mozilla.org/v1/` — is duplicated across
crates with different patterns.

The macro provides:

1. **Deduplication leverage** — a single canonical way to declare a hard-coded URL.
2. **Parseability proof at build time** — typos in URL literals fail `cargo build`, not production.
3. **Zero runtime cost beyond the existing `Url::parse`** — the expansion is what your code already does.

## What it is *not*

- **Not a `const` URL** — `url::Url::parse` is not `const fn`. The macro
  cannot be used in `static FOO: Url = url!(...)`. Wrap with
  `once_cell::Lazy` or `std::sync::LazyLock` for static contexts.
- **Not a substitute for semantic tests** — the macro proves a string
  parses as a URL. It does not prove the URL points to the correct
  server, has the right scheme, or matches your intent. Existing tests
  that assert host/path/scheme should stay.

## API

| Form | Behavior |
|---|---|
| `url!("https://example.com/")` | Returns `url::Url`. |
| `url!("malformed")` | Compile error with `url::ParseError`'s message. |
| `url!()` / `url!(123)` | Compile error: expected string literal. |

The expanded code references `::url::Url::parse`, so the calling crate
must declare `url` as a dependency.

## Adoption

**Opt-in, per-component, no deadline.** The crate is being introduced
in `ads-client` first as a working example. Other components are welcome
to migrate at their own pace.

### Before / after (from ads-client)

```rust
// Before
static MARS_API_ENDPOINT_PROD: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://ads.mozilla.org/v1/").expect("hardcoded URL must be valid")
});

// After
static MARS_API_ENDPOINT_PROD: Lazy<Url> = Lazy::new(|| url!("https://ads.mozilla.org/v1/"));
```

### Migration recipe

1. Add `url-macro = { path = "../support/url-macro" }` to your crate's
   `Cargo.toml` `[dependencies]`.
2. `use url_macro::url;` in any module with hard-coded URL literals.
3. Replace `Url::parse("...").expect(...)` (or `.unwrap()`) with
   `url!("...")`. Drop the `.expect`/`.unwrap` — the macro guarantees
   the parse cannot fail.
4. **Do not migrate**:
   - Test fixture URLs where panic on bad input is the desired behavior.
   - Dynamic URLs built via `format!` or runtime concatenation — the
     macro only accepts string literals.

## Status

Initial release — `url!` macro only. Future extensions (e.g.,
`base_url!` returning a newtype, scheme-restricted variants) will be
proposed via separate ADRs if usage warrants.

## Layout

```
components/support/url-macro/
├── Cargo.toml          # proc-macro = true
├── src/lib.rs          # url! implementation (~30 LOC, syn + quote)
├── README.md           # this file
└── tests/              # separate test crate (url-macro-tests)
    ├── Cargo.toml
    ├── tests.rs        # trybuild driver
    ├── pass.rs
    ├── *.rs            # compile-fail cases
    └── *.stderr        # trybuild golden files
```

## Questions / migrating your component?

Reach out in the application-services repo. The reference migration is
`components/ads-client/src/mars/environment.rs`.
