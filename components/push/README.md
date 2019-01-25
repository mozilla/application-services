# Rust Push Component

This is a companion library for the work being proposed for the Rust
Push Component. This skeleton is very much incomplete and subject to
drastic change.


The code was derived from the `mozilla-central/dom/push/` directory
and best estimates were used to determine types and structures. Note
that `unknown.rs` contains structres that could not be readily
determined. These must be resolved before meaningful work on this API
can continue.

In many instances, best guesses were made for the return types and
functions (e.g. the original code makes heavy use of Javascript
Promise objects, which have no analog in Rust. These were converted to
rust `futures`)

Note: we've been encouraged to model after the "places" component.
this means defining the final Push API elements as kotlin in the
android directory ffi descriptions. Since this could cause compile
failures, it's currently not checked in.
