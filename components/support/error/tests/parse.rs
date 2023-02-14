/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt::Display;

use error_support::{handle_error, ErrorHandling, GetErrorHandling};

#[derive(Debug, thiserror::Error)]
struct Error {}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Internal Error!")
    }
}

#[derive(Debug, thiserror::Error)]
struct ExternalError {}

impl Display for ExternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "External Error!")
    }
}

impl GetErrorHandling for Error {
    type ExternalError = ExternalError;

    fn get_error_handling(&self) -> error_support::ErrorHandling<Self::ExternalError> {
        ErrorHandling::convert(ExternalError {})
    }
}

#[handle_error(Error)]
fn func() -> ::std::result::Result<String, ExternalError> {
    Err(Error{})
}

#[handle_error(Error)]
fn func2() -> Result<String, ExternalError> {
    Err(Error{})
}

type MyResult<T, E = ExternalError> = std::result::Result<T, E>;

#[handle_error(Error)]
fn func3() -> MyResult<String> {
    Err(Error{})
}

type FullyAliasedResult<T = String, E = ExternalError> = std::result::Result<T, E>;

#[handle_error(Error)]
fn func4() -> FullyAliasedResult {
    Err(Error{})
}

mod submodule {
    #[super::handle_error(super::Error)]
    pub(super) fn func() -> ::std::result::Result<String, super::ExternalError> {
        Err(super::Error{})
    }
}

fn main() {
    // We verify that all functions now return Result<T, ExternalError>
    let _: Vec<ExternalError> = vec![
        func().unwrap_err(),
        func2().unwrap_err(),
        func3().unwrap_err(),
        func4().unwrap_err(),
        submodule::func().unwrap_err(),
    ];
}
