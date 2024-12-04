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

#[handle_error(Error)] // Works.
fn func() -> ::std::result::Result<String, ExternalError> {
    Err(Error{})
}

#[handle_error("Error")] // Quoted string instead of a path
fn func() -> ::std::result::Result<String, ExternalError> {
    Err(Error{})
}

#[handle_error(2)] // bad type.
fn func() -> ::std::result::Result<String, ExternalError> {
    Err(Error{})
}

#[handle_error] // No args.
fn func() -> ::std::result::Result<String, ExternalError> {
    Err(Error{})
}

#[handle_error()] // empty args.
fn func() -> ::std::result::Result<String, ExternalError> {
    Err(Error{})
}

#[handle_error(A, B)] // too many args.
fn func() -> ::std::result::Result<String, ExternalError> {
    Err(Error{})
}

#[handle_error(Key="Value")] // unknown args.
fn func() -> ::std::result::Result<String, ExternalError> {
    Err(Error{})
}

// When the "external" error doesn't implement `std::error::Error` (eg, we use String) but the
// "inner" one does.
#[derive(Debug, thiserror::Error)]
struct Error2 {
    string: String,
}

impl Display for Error2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "String Error!")
    }
}

impl GetErrorHandling for Error2 {
    type ExternalError = String;

    fn get_error_handling(&self) -> error_support::ErrorHandling<Self::ExternalError> {
        ErrorHandling::convert(self.string.clone())
    }
}

#[handle_error(Error2)] // Must implement `std::error::Error`
fn func_error_to_string() -> Result<String, String> {
    Err(Error2 { string: "oops!".into() })
}

// When the "external" error does implement `std::error::Error` but the
// "inner" one does not.
#[derive(Debug, thiserror::Error)]
struct ExternalError2 {}

impl Display for ExternalError2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "External Error")
    }
}

// So we can't even try to `impl GetErrorHandling for String` as String isn't
// in this crate!
// So the output complains about *both* that trait missing *and* the lack of `std::error::Error`
#[handle_error(String)] // Must implement `std::error::Error`
fn func_string_to_error() -> Result<String, ExternalError2> {
    Err("oops!".into())
}

fn main(){}
