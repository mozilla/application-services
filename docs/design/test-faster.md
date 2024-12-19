# Testing faster: How to avoid making compile times worse by adding tests

## Background

We'd like to keep `cargo test`, `cargo build`, `cargo check`, ... reasonably
fast, and we'd *really* like to keep them fast if you pass `-p` for a specific
project. Unfortunately, there are a few ways this can become unexpectedly slow.
The easiest of these problems for us to combat at the moment is the unfortunate
placement of dev-dependencies in our build graph.

If you perform a `cargo test -p foo`, all dev-dependencies of `foo` must be
compiled before `foo`'s tests can start. This includes dependencies only used
non-test targets, such as examples or benchmarks.

In an ideal world, cargo could run your tests as soon as it finished with the
dependencies it needs for those tests, instead of waiting for your benchmark
suite, or the arg-parser your examples use, or etc.

Unfortunately, all cargo knows is that these are `dev-dependencies`, and not
which targets actually use them.

Additionally, unqualified invocations of cargo (that is, without `-p`) might
have an even worse time if we aren't careful. If I run, `cargo test`, cargo
knows *every* crate in the workspace needs to be built with all dev
dependencies, if `places` depends on `fxa-client`, all of `fxa-clients`
dev-dependencies must be compiled, ready, and linked in at least to the `lib`
target before we can even think about starting on `places`.

We have not been careful about what shape the dependency graph ends up as when example code is
taken into consideration (as it is by cargo during certain builds), and as a
result, we have this problem. Which isn't really a problem we
want to fix: Example code can and should depend on several different components,
and use them together in interesting ways.

So, because we don't want to change what our examples do, or make
major architectural changes of the non-test code for something like this, we
need to do something else.

## The Solution

To fix this, we manually insert "cuts" into the dependency graph to help cargo
out. That is, we pull some of these build targets (e.g. examples, benchmarks,
tests if they cause a substantial compile overhead) into their own dedicated
crates so that:

1. They can be built in parallel with each other.
2. Crates depending on the component itself are not waiting on the
   test/bench/example build in order for their test build to begin.
3. A potentially smaller set of our crates need to be rebuilt -- and a smaller
   set of possible configurations exist meaning fewer items to add pressure to
   caches.
4. ...

Some rules of thumb for when / when not to do this:

- All rust examples should be put in `examples/*`.

- All rust benchmarks should be put in `testing/separated/*`. See the section
  below on how to set your benchmark up to avoid redundant compiles.

- Rust tests which brings in heavyweight dependencies should be evaluated on an
  ad-hoc basis. If you're concerned, measure how long compilation takes
  with/without, and consider how many crates depend on the crate where the test
  lives (e.g. a slow test in support/foo might be far worse than one in a leaf
  crate), etc...

### Appendix: How to avoid redundant compiles for benchmarks and integration tests

To be clear, this is way more important for benchmarks (which always compile as
release and have a costly link phase).


Say you have a directory structure like the following:

```
mycrate
 ├── src
 │   └── lib.rs
 | ...
 ├── benches
 │   ├── bench0.rs
 |   ├── bench1.rs
 │   └── bench2.rs
 ├── tests
 │   ├── test0.rs
 |   ├── test1.rs
 │   └── test2.rs
 └── ...
```

When you run your integration tests or benchmarks, each of `test0`, `test1`,
`test2` or `bench0`, `bench1`, `bench2` is compiled as it's own crate that runs
the tests in question and exits.

That means 3 benchmark executables are built on release settings, and 3
integration test executables.

If you've ever tried to add a piece of shared utility code into your integration
tests, only to have cargo (falsely) complain that it is dead code: this is why.
Even if `test0.rs` and `test2.rs` *both* use the utility function, unless
*every* test crate uses *every* shared utility, the crate that doesn't will
complain.

(Aside: This turns out to be an unintentional secondary benefit of this approach
-- easier shared code among tests, without having to put a
`#![allow(dead_code)]` in your utils.rs. We haven't hit that very much here,
since we tend to stick to unit tests, but it came up in mentat several times,
and is a frequent complaint people have)

Anyway, the solution here is simple: Create a new crate. If you were working in
`components/mycrate` and you want to add some integration tests or benchmarks,
you should do `cargo new --lib testing/separated/mycrate-test` (or
`.../mycrate-bench`).

Delete `.../mycrate-test/src/lib.rs`. Yep, really, we're making a crate that
only has integration tests/benchmarks (See the "FAQ0" section at the bottom of
the file if you're getting incredulous).

Now, add a `src/tests.rs` or a `src/benches.rs`. This file should contain `mod
foo;` declarations for each submodule containing tests/benchmarks, if any.

For benches, this is also where you set up the benchmark harness (refer to
benchmark library docs for how).

Now, for a test, add: into your Cargo.toml

```toml
[[test]]
name = "mycrate-test"
path = "src/tests.rs"
```

and for a benchmark, add:

```toml
[[bench]]
name = "mycrate-benches"
path = "src/benches.rs"
harness = false
```

Because we aren't using `src/lib.rs`, this is what declares which file is the
root of the test/benchmark crate. Because there's only one target (unlike with
`tests/*` / `benches/*` under default settings), this will compile more quickly.

Additionally, `src/tests.rs` and `src/benches.rs` will behave like a normal
crate, the only difference being that they don't produce a lib, and that they're
triggered by `cargo test`/`cargo run` respectively.


### FAQ0: Why put tests/benches in `src/*` instead of disabling `autotests`/`autobenches`

Instead of putting tests/benchmarks inside `src`, we could just delete the `src`
dir outright, and place everything in `tests`/`benches`.

Then, to get the same one-rebuild-per-file behavior that we'll get in `src`, we
need to add `autotests = false` or `autobenches = false` to our Cargo.toml,
adding a root `tests/tests.rs` (or `benches/benches.rs`) containing `mod` decls
for all submodules, and finally by referencing that "root" in the Cargo.toml
`[[tests]]` / `[[benches]]` list, exactly the same way we did for using `src/*`.

This would work, and on the surface, using `tests/*.rs` and `benches/*.rs` seems
more consistent, so it seems weird to use `src/*.rs` for these files.

My reasoning is as follows: Almost universally, `tests/*.rs`, `examples/*.rs`,
`benches/*.rs`, etc. are automatic. If you add a test into the tests folder, it
will run without anything else.

If we're going to set up one-build-per-{test,bench}suite as I described, this
fundamentally cannot be true. In this paradigm, if you add a test file named
`blah.rs`, you must add a `mod blah` it to the parent module.

It seems both confusing and error-prone to use `tests/*`, but have it behave
that way, however this is absolutely the normal behavior for files in `src/*.rs`
-- When you add a file, you then need to add it to it's parent module, and this
is something Rust programmers are pretty used to.

(In fact, we even replicated this behavior (for no reason) in the places
integration tests, and added the `mod` declarations to a "controlling" parent
module -- It seems weird to be in an environment where this *isn't* required)

So, that's why. This way, we make it *way* less likely that you add a test file
to some directory, and have it get ignored because you didn't realize that in
this one folder, you need to add a `mod mytest` into a neighboring tests.rs.
