// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

#![allow(dead_code)]

use std; // To refer to std::result::Result.

use mentat::{
    MentatError,
};
use failure::Fail;

pub type Result<T> = std::result::Result<T, Error>;

#[macro_export]
macro_rules! bail {
    ($e:expr) => (
        return Err($e.into());
    )
}

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "bad query result type")]
    BadQueryResultType,

    #[fail(display = "{}", _0)]
    MentatError(#[cause] MentatError),
}

// Because Mentat doesn't expose its entire API from the top-level `mentat` crate, we sometimes
// witness error types that are logically subsumed by `MentatError`.  We wrap those here, since
// _our_ consumers should not care about the specific Mentat error type.
impl<E: Into<MentatError> + std::fmt::Debug> From<E> for Error {
    fn from(error: E) -> Error {
        error!("MentatError -> LoginsError {:?}", error);
        let mentat_err: MentatError = error.into();
        if let Some(bt) = mentat_err.backtrace() {
            debug!("Backtrace: {:?}", bt);
        }
        Error::MentatError(mentat_err)
    }
}
