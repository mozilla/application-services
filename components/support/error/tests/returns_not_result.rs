/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

 use std::fmt::Display;

 use error_support::{handle_error, ErrorHandling, GetErrorHandling};

 #[derive(Debug, thiserror::Error)]
 struct Error {}
 impl Display for Error {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

 #[derive(Debug, thiserror::Error)]
 struct ExternalError {}

 impl Display for ExternalError {
     fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
         Ok(())
     }
 }

 impl GetErrorHandling for Error {
     type ExternalError = ExternalError;

     fn get_error_handling(&self) -> error_support::ErrorHandling<Self::ExternalError> {
         ErrorHandling::convert(ExternalError {})
     }
 }

 // This function says it should return a string and
 // it's body returns a Result<String, Error>. The error will be mapped to `ExternalError`
 // using the macro
 // however, the compiler will error because the return type is still a String
 // and the function after the macro returns `Result<String,ExternalError>`
 #[handle_error(Error)]
 fn func() -> String {
     Ok("".to_string())
 }

 fn main() {}
