/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Crash Test Helper APIs
//!
//! The `crashtest` component offers a little helper API that lets you deliberately
//! crash the application. It's intended to help developers test the crash-handling
//! and crash-reporting capabilities of their app.

// Temporary, to work around a clippy lint in generated code.
// https://github.com/mozilla/uniffi-rs/issues/1018
#![allow(clippy::redundant_closure)]

use thiserror::Error;

#[cfg(test)]
mod tests;

uniffi::include_scaffolding!("crashtest");

/// Trigger a hard abort inside the Rust code.
///
/// This function simulates some kind of uncatchable illegal operation
/// performed inside the Rust code. After calling this function you should
/// expect your application to be halted with e.g. a `SIGABRT` or similar.
///
pub fn trigger_rust_abort() {
    log::error!("Now triggering an abort inside the Rust code");
    std::process::abort();
}

/// Trigger a panic inside the Rust code.
///
/// This function simulates the occurrence of an unexpected state inside
/// the Rust code that causes it to panic. We build our Rust components to
/// unwind on panic, so after calling this function through the foreign
/// language bindings, you should expect it to intercept the panic translate
/// it into some foreign-language-appropriate equivalent:
///
///  - In Kotlin, it will throw an exception.
///  - In Swift, it will fail with a `try!` runtime error.
///
pub fn trigger_rust_panic() {
    log::error!("Now triggering a panic inside the Rust code");
    panic!("Panic! In The Rust Code.");
}

/// Trigger an error inside the Rust code.
///
/// This function simulates the occurrence of an expected error inside
/// the Rust code. You should expect calling this function to throw the
/// foreign-language representation of the [`CrashTestError`] class.
///
pub fn trigger_rust_error() -> Result<(), CrashTestError> {
    log::error!("Now triggering an error inside the Rust code");
    Err(CrashTestError::ErrorFromTheRustCode)
}

/// An error that can be returned from Rust code.
///
#[derive(Debug, Error)]
pub enum CrashTestError {
    #[error("Error! From The Rust Code.")]
    ErrorFromTheRustCode,
}
