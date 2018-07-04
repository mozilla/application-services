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

use serde_json;

use mentat;
use logins;
use sync15_adapter;

pub type Result<T> = std::result::Result<T, Sync15PasswordsError>;

#[macro_export]
macro_rules! bail {
    ($e:expr) => (
        return Err($e.into());
    )
}

#[derive(Debug, Fail)]
pub enum Sync15PasswordsError {
    #[fail(display = "{}", _0)]
    MentatError(#[cause] mentat::MentatError),

    #[fail(display = "{}", _0)]
    LoginsError(#[cause] logins::Error),

    #[fail(display = "{}", _0)]
    Sync15AdapterError(#[cause] sync15_adapter::Error),

    #[fail(display = "{}", _0)]
    SerdeJSONError(#[cause] serde_json::Error),
}

impl From<mentat::MentatError> for Sync15PasswordsError {
    fn from(error: mentat::MentatError) -> Sync15PasswordsError {
        Sync15PasswordsError::MentatError(error)
    }
}

impl From<logins::Error> for Sync15PasswordsError {
    fn from(error: logins::Error) -> Sync15PasswordsError {
        Sync15PasswordsError::LoginsError(error)
    }
}

impl From<sync15_adapter::Error> for Sync15PasswordsError {
    fn from(error: sync15_adapter::Error) -> Sync15PasswordsError {
        Sync15PasswordsError::Sync15AdapterError(error)
    }
}

impl From<serde_json::Error> for Sync15PasswordsError {
    fn from(error: serde_json::Error) -> Sync15PasswordsError {
        Sync15PasswordsError::SerdeJSONError(error)
    }
}
