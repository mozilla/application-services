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

#[handle_error(ExternalError)]
fn func() -> ::std::result::Result<String, Error> {
    Err(Error{})
}

#[handle_error(ExternalError)]
fn func2() -> Result<String, Error> {
    Err(Error{})
}

type MyResult<T, E = Error> = std::result::Result<T, E>;

#[handle_error(ExternalError)]
fn func3() -> MyResult<String> {
    Err(Error{})
}

fn main() {
    // We verify that all functions now return Result<T, ExternalError>
    let _: Vec<ExternalError> = vec![
        func().unwrap_err(), func2().unwrap_err(), func3().unwrap_err()
    ];
}
