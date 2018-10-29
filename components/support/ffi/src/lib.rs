/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # FFI Support
//!
//! This crate implements a support library to simplify implementing the patterns that the
//! `mozilla/application-services` repository uses for it's "Rust Component" FFI libraries.
//!
//! It is *strongly encouraged* that anybody writing FFI code in this repository read this
//! documentation before doing so, as it is a subtle, difficult, and error prone process.
//!
//! ## Terminology
//!
//! For each library, there are currently three parts we're concerned with. There's no clear correct
//! name for these, so this documentation will attempt to use the following terminology:
//!
//! - **Rust Component**: A Rust crate which does not expose an FFI directly, but may be may be
//!   wrapped by one that does. These have a `crate-type` in their Cargo.toml (see
//!   https://doc.rust-lang.org/reference/linkage.html) of `lib`, and not `staticlib` or `cdylib`
//!   (Note that `lib` is the default if `crate-type` is not specified). Examples include the
//!   `fxa-client`, and `logins` crates.
//!
//! - **FFI Component**: A wrapper crate that takes a Rust component, and exposes an FFI from it.
//!   These typically have `ffi` in the name, and have `crate-type = ["lib", "staticlib", "cdylib"]`
//!   in their Cargo.toml. For example, the `fxa-client/ffi` and `logins/ffi` crates (note:
//!   paths are subject to change). When built, these produce a native library that is consumed by
//!   the "FFI Consumer".
//!
//! - **FFI Consumer**: This is a low level library, typically implemented in Kotlin (for Android)
//!   or Swift (for iOS), that exposes a memory-safe wrapper around the memory-unsafe C API produced
//!   by the FFI component. It's expected that the maintainers of the FFI Component and FFI Consumer
//!   be the same (or at least, the author of the consumer should be completely comfortable with the
//!   API exposed by, and code in the FFI component), since the code in these is extremely tightly
//!   coupled, and very easy to get wrong.
//!
//! Note that while there are three parts, there may be more than three libraries relevant here, for
//! example there may be more than one FFI consumer (one for Android, one for iOS).
//!
//! ## Usage
//!
//! This library will typically be used in both the Rust component, and the FFI component, however
//! it frequently will be an optional dependency in the Rust component that's only available when a
//! feature flag (which the FFI component will always require) is used.
//!
//! The reason it's required inside the Rust component (and not solely in the FFI component, which
//! would be nice), is so that types provided by that crate may implement the traits provided by
//! this crate (this is because Rust does not allow crate `C` to implement a trait defined in crate
//! `A` for a type defined in crate `B`).
//!
//! In general, examples should be provided for the most important types and functions
//! ([`call_with_result`], [`IntoFfi`],
//! [`ExternError`], etc), but you should also look at the code of
//! consumers of this library.
//!
//! ### Usage in the Rust Component
//!
//! Inside the Rust component, you will implement:
//!
//! 1. [`IntoFfi`] for all types defined in that crate that you want to return
//!    over the FFI. For most common cases, the [`implement_into_ffi_by_pointer!`] and
//!    [`implement_into_ffi_by_json!`] macros will do the job here, however you can see that trait's
//!    documentation for discussion and examples of implementing it manually.
//!
//! 2. Conversion to [`ExternError`] for the error type(s) exposed by that
//!    rust component, that is, `impl From<MyError> for ExternError`.
//!
//! ### Usage in the FFI Component
//!
//! Inside the FFI component, you will use this library in a few ways:
//!
//! 1. Destructors will be exposed for each types that had [`implement_into_ffi_by_pointer!`] called
//!    on it (using [`define_box_destructor!`]), and a destructor for strings should be exposed as
//!    well, using [`define_string_destructor`]
//!
//! 2. The body of every / nearly every FFI function will be wrapped in either a
//!    [`call_with_result`] or [`call_with_output`].
//!
//!    This is required because if we `panic!` (e.g. from an `assert!`, `unwrap()`, `expect()`, from
//!    indexing past the end of an array, etc) across the FFI boundary, the behavior is undefined
//!    and in practice very weird things tend to happen (we aren't caught by the caller, since they
//!    don't have the same exception behavior as us).
//!
//!    If you don't think your program (or possibly just certain calls) can handle panics, you may
//!    also use the versions of these functions in the [`abort_on_panic`] module, which
//!    do as their name suggest.
//!
//! Additionally, c strings that are passed in as arguments may be converted to rust strings using
//! helpers such as [`rust_str_from_c`], [`opt_rust_str_from_c`], [`rust_string_from_c`],
//! [`opt_rust_string_from_c`], etc.
//!

use std::{panic, thread};

mod error;
pub mod handle_map;
mod into_ffi;
mod macros;
mod string;

pub use crate::error::*;
pub use crate::into_ffi::*;
pub use crate::macros::*;
pub use crate::string::*;

// We export most of the types from this, but some constants
// (MAX_CAPACITY) don't make sense at the top level.
pub use crate::handle_map::{ConcurrentHandleMap, Handle, HandleError, HandleMap};

/// Call a callback that returns a `Result<T, E>` while:
///
/// - Catching panics, and reporting them to C via [`ExternError`].
/// - Converting `T` to a C-compatible type using [`IntoFfi`].
/// - Converting `E` to a C-compatible error via `Into<ExternError>`.
///
/// This (or [`call_with_output`]) should be in the majority of the FFI functions, see the crate
/// top-level docs for more info.
///
/// If your function doesn't produce an error, you may use [`call_with_output`] instead, which
/// doesn't require you return a Result.
///
/// ## Example
///
/// A few points about the following example:
///
/// - This function *must* be unsafe, as it reads from a raw pointer. If you made it safe, then safe
///   Rust could cause memory safety violations, which would be very bad! (However, FFI functions
///   that don't read from raw pointers don't need to be marked `unsafe`! Sadly, most of ours need
///   to take strings, and so we're out of luck...)
///
/// - We need to mark it as `#[no_mangle] pub extern "C"`.
///
/// - We prefix it with a unique name for the library (e.g. `mylib_`). Foreign functions are not
///   namespaced, and symbol collisions can cause a large number of problems and subtle bugs,
///   including memory safety issues in some cases.
///
/// ```rust,no_run
/// # use ffi_support::{ExternError, ErrorCode};
/// # use std::os::raw::c_char;
///
/// # #[derive(Debug)]
/// # struct BadEmptyString;
/// # impl From<BadEmptyString> for ExternError {
/// #     fn from(e: BadEmptyString) -> Self {
/// #         ExternError::new_error(ErrorCode::new(1), "Bad empty string")
/// #     }
/// # }
///
/// #[no_mangle]
/// pub unsafe extern "C" fn mylib_print_string(
///     // Strings come in as a null terminated C string. This is certainly not ideal but it simplifies
///     // the "FFI consumer" code, which is trickier code to get right, as it typically has poor
///     // support for interacting with native libraries.
///     thing_to_print: *const c_char,
///     // Note that taking `&mut T` and `&T` is both allowed and encouraged, so long as `T: Sized`,
///     // (e.g. it can't be a trait object, `&[T]`, a `&str`, etc). Also note that `Option<&T>` and
///     // `Option<&mut T>` are also allowed, if you expect the caller to sometimes pass in null, but
///     // that's the only case when it's currently to use `Option` in an argument list like this).
///     error: &mut ExternError
/// ) {
///     // You should try to to do as little as possible outside the call_with_result,
///     // to avoid a case where a panic occurs.
///     ffi_support::call_with_result(error, || {
///         let s = ffi_support::rust_str_from_c(thing_to_print);
///         if s.len() == 0 {
///             // This is a silly example!
///             return Err(BadEmptyString);
///         }
///         println!("{}", s);
///         Ok(())
///     })
/// }
/// ```
///
/// ## Unwind (panic) Safety
///
/// Internally, this function wraps it's argument in a
/// [`AssertUnwindSafe`](std::panic::AssertUnwindSafe). That means it doesn't attempt to force you
/// to mark things as [`UnwindSafe`](std::panic::UnwindSafe). Effectively, we're saying that every
/// caller to this function is automatically panic safe, which is a lie. This is not ideal, but it's
/// unclear what the right call here would be.
///
/// To be clear, making the wrong choice here has no bearing on memory safety, unless there are
/// exisiting memory safety holes in the code. That means by using `AssertUnwindSafe`, we end up in
/// a position closer to the position we'd be in if we were working in a language with exceptions,
/// which typically provides little-to-no assistance in terms of program correctness in the case of
/// something `throw`ing.
///
/// Anyway, if we *were* to require `F: UnwindSafe`, the implementer of the FFI component would need
/// to use `AssertUnwindSafe` on every FFI binding that wraps a method that needs to call something
/// on a `&mut T` (note that this is *not* true for `*mut T`, which we want to discourage). The use
/// of this seems likely to be frequent enough in this FFI that I have an extremely hard time
/// believing it would be used with consideration, so while the strategy of "assume everything is
/// panic-safe" is clearly not great, it seems likely to be what happens anyway.
///
/// There are, of course, other options:
///
/// 1. Abort on panic (e.g. only expose the implementations in `abort_on_panic`), which is bad
///    for obvious reasons, and seems even worse given our position as libraries.
/// 2. Poison on panic (as [`std::sync::Mutex`] does, for example). This is a valid option, but
///    seems wrong for all cases.
/// 3. Re-initialize on panic (e.g. reopen the DB connection).
///
/// 2 and 3 are promising, and allowing users of `ffi-support` to make these choices with a low
/// amount of boilerplate is something we'd like to investigate in the future, but currently this
/// is where we've landed.
pub fn call_with_result<R, E, F>(out_error: &mut ExternError, callback: F) -> R::Value
where
    F: FnOnce() -> Result<R, E>,
    // It would be nice to only require std::fmt::Debug if the `log_backtraces`
    // feature is on, but there's not really a way to do that in stable rust (at least
    // not in a way that wouldn't add more work for consumers of this lib).
    E: Into<ExternError> + std::fmt::Debug,
    R: IntoFfi,
{
    call_with_result_impl(out_error, callback, false)
}

/// Call a callback that returns a `T` while:
///
/// - Catching panics, and reporting them to C via [`ExternError`]
/// - Converting `T` to a C-compatible type using [`IntoFfi`]
///
/// Note that you still need to provide an [`ExternError`] to this function, to report panics.
///
/// See [`call_with_result`] if you'd like to return a `Result<T, E>` (Note: `E` must
/// be convertible to [`ExternError`]).
///
/// This (or [`call_with_result`]) should be in the majority of the FFI functions, see
/// the crate top-level docs for more info.
pub fn call_with_output<R, F>(out_error: &mut ExternError, callback: F) -> R::Value
where
    F: FnOnce() -> R,
    R: IntoFfi,
{
    // We need something that's `Into<ExternError>`, even though we never return it, so just use
    // `ExternError` itself.
    call_with_result(out_error, || -> Result<_, ExternError> { Ok(callback()) })
}

fn call_with_result_impl<R, E, F>(
    out_error: &mut ExternError,
    callback: F,
    abort_on_panic: bool,
) -> R::Value
where
    F: FnOnce() -> Result<R, E>,
    E: Into<ExternError> + std::fmt::Debug,
    R: IntoFfi,
{
    *out_error = ExternError::success();
    // It's not ideal to handle unwind safety this way, however I'm not sure we can reasonably
    // expect the FFI code to think about this in a meaningful way. That said, you cannot cause
    // memory safety violations by breaking unwind safety (note that this function is not `unsafe`),
    // short of bugs in unsafe code elsewhere, so this isn't the *worst* thing we could be doing.
    let res: thread::Result<(ExternError, R::Value)> =
        panic::catch_unwind(panic::AssertUnwindSafe(|| {
            init_backtraces_once();
            match callback() {
                Ok(v) => (ExternError::default(), v.into_ffi_value()),
                Err(e) => (e.into(), R::ffi_default()),
            }
        }));
    match res {
        Ok((err, o)) => {
            *out_error = err;
            o
        }
        Err(e) => {
            log::error!("Caught a panic calling rust code: {:?}", e);
            if abort_on_panic {
                std::process::abort();
            }
            *out_error = e.into();
            R::ffi_default()
        }
    }
}

/// This module exists just to expose a variant of [`call_with_result`] and [`call_with_output`]
/// that aborts, instead of unwinding, on panic.
pub mod abort_on_panic {
    use super::*;

    /// Same as the root `call_with_result`, but aborts on panic instead of unwinding. See the
    /// `call_with_result` documentation for more.
    pub fn call_with_result<R, E, F>(out_error: &mut ExternError, callback: F) -> R::Value
    where
        F: FnOnce() -> Result<R, E>,
        E: Into<ExternError> + std::fmt::Debug,
        R: IntoFfi,
    {
        super::call_with_result_impl(out_error, callback, true)
    }

    /// Same as the root `call_with_output`, but aborts on panic instead of unwinding. As a result,
    /// it doesn't require a [`ExternError`] out argument. See the `call_with_output` documentation
    /// for more info.
    pub fn call_with_output<R, F>(callback: F) -> R::Value
    where
        F: FnOnce() -> R,
        R: IntoFfi,
    {
        let mut dummy = ExternError::success();
        super::call_with_result_impl(
            &mut dummy,
            || -> Result<_, ExternError> { Ok(callback()) },
            true,
        )
    }
}

#[cfg(feature = "log_backtraces")]
fn init_backtraces_once() {
    use std::sync::{Once, ONCE_INIT};
    static INIT_BACKTRACES: Once = ONCE_INIT;
    INIT_BACKTRACES.call_once(move || {
        // Turn on backtraces for failure, if it's still listening.
        std::env::set_var("RUST_BACKTRACE", "1");
        // Turn on a panic hook which logs both backtraces and the panic
        // "Location" (file/line). We do both in case we've been stripped,
        // ).
        std::panic::set_hook(Box::new(move |panic_info| {
            let (file, line) = if let Some(loc) = panic_info.location() {
                (loc.file(), loc.line())
            } else {
                // Apparently this won't happen but rust has reserved the
                // ability to start returning None from location in some cases
                // in the future.
                ("<unknown>", 0)
            };
            log::error!("### Rust `panic!` hit at file '{}', line {}", file, line);
            // We could use failure for failure::Backtrace (and we enable RUST_BACKTRACE
            // to opt-in to backtraces on failure errors if possible), however:
            // - we don't already have a failure dependency (one is likely inevitable,
            //   and all our clients do, so this doesn't matter)
            // - `failure` only checks the RUST_BACKTRACE variable once, and we could have errors
            //   before this. So we just use the backtrace crate directly.
            log::error!("  Complete stack trace:\n{:?}", backtrace::Backtrace::new());
        }));
    });
}

#[cfg(not(feature = "log_backtraces"))]
fn init_backtraces_once() {}
