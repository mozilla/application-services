/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

 use std::fmt::Display;

 use error_support::{handle_error, ErrorHandling, GetErrorHandling};

 #[derive(Debug, thiserror::Error)]
 enum Error {}


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

 // This is OK
 #[handle_error(Error)]
 fn func() -> ::std::result::Result<String, ExternalError> {
     Ok("".to_string())
 }


 // handle error only works on functions! this will fail to compile
 #[handle_error(Error)]
 struct SomeType {}

 #[handle_error(Error)]
 const A: u32 = 0;

 #[handle_error(Error)]
 impl SomeType {}

 fn main() {}
