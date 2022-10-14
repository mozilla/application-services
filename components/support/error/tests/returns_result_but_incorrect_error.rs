/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

 use std::fmt::Display;

 use error_support::{handle_error, ErrorHandling, GetErrorHandling};
 
 #[derive(Debug, thiserror::Error)]
 enum Error {}
 
 
 #[derive(Debug, thiserror::Error)]
 struct ExternalError {}

 #[derive(Debug, thiserror::Error)]
 struct OtherExternalError {}

 impl Display for ExternalError {
     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
         Ok(())
     }
 }


 impl Display for OtherExternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}
 
 impl GetErrorHandling for Error {
     type ExternalError = OtherExternalError;
 
     fn get_error_handling(&self) -> error_support::ErrorHandling<Self::ExternalError> {
         ErrorHandling::convert(OtherExternalError {})
     }
 }

 // handle_error expects that `Error` implements GetErrorHandling<E = ExternalError>
 // instead, it implements GetErrorHandling<E = OtherExternalError>
 #[handle_error(ExternalError)]
 fn func() -> Result<String, Error> {
     Ok("".to_string())
 }

 fn main() {}
