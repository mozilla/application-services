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
use failure::{Context, Backtrace, Fail};

pub type Result<T> = std::result::Result<T, Sync15PasswordsError>;

#[macro_export]
macro_rules! bail {
    ($e:expr) => (
        return Err($e.into());
    )
}

#[derive(Debug)]
pub struct Sync15PasswordsError(Box<Context<Sync15PasswordsErrorKind>>);

impl Fail for Sync15PasswordsError {
    #[inline]
    fn cause(&self) -> Option<&Fail> {
        self.0.cause()
    }

    #[inline]
    fn backtrace(&self) -> Option<&Backtrace> {
        self.0.backtrace()
    }
}

impl std::fmt::Display for Sync15PasswordsError {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(&*self.0, f)
    }
}

impl Sync15PasswordsError {
    #[inline]
    pub fn kind(&self) -> &Sync15PasswordsErrorKind {
        &*self.0.get_context()
    }
}

impl From<Sync15PasswordsErrorKind> for Sync15PasswordsError {
    #[inline]
    fn from(kind: Sync15PasswordsErrorKind) -> Sync15PasswordsError {
        Sync15PasswordsError(Box::new(Context::new(kind)))
    }
}

impl From<Context<Sync15PasswordsErrorKind>> for Sync15PasswordsError {
    #[inline]
    fn from(inner: Context<Sync15PasswordsErrorKind>) -> Sync15PasswordsError {
        Sync15PasswordsError(Box::new(inner))
    }
}

#[derive(Debug, Fail)]
pub enum Sync15PasswordsErrorKind {
    #[fail(display = "{}", _0)]
    MentatError(#[cause] mentat::MentatError),

    #[fail(display = "{}", _0)]
    LoginsError(#[cause] logins::Error),

    #[fail(display = "{}", _0)]
    Sync15AdapterError(#[cause] sync15_adapter::Error),

    #[fail(display = "{}", _0)]
    SerdeJSONError(#[cause] serde_json::Error),
}

impl From<mentat::MentatError> for Sync15PasswordsErrorKind {
    fn from(error: mentat::MentatError) -> Sync15PasswordsErrorKind {
        Sync15PasswordsErrorKind::MentatError(error)
    }
}

impl From<logins::Error> for Sync15PasswordsErrorKind {
    fn from(error: logins::Error) -> Sync15PasswordsErrorKind {
        Sync15PasswordsErrorKind::LoginsError(error)
    }
}

impl From<sync15_adapter::Error> for Sync15PasswordsErrorKind {
    fn from(error: sync15_adapter::Error) -> Sync15PasswordsErrorKind {
        Sync15PasswordsErrorKind::Sync15AdapterError(error)
    }
}

impl From<serde_json::Error> for Sync15PasswordsErrorKind {
    fn from(error: serde_json::Error) -> Sync15PasswordsErrorKind {
        Sync15PasswordsErrorKind::SerdeJSONError(error)
    }
}

impl From<mentat::MentatError> for Sync15PasswordsError {
    fn from(error: mentat::MentatError) -> Sync15PasswordsError {
        Sync15PasswordsErrorKind::from(error).into()
    }
}

impl From<logins::Error> for Sync15PasswordsError {
    fn from(error: logins::Error) -> Sync15PasswordsError {
        Sync15PasswordsErrorKind::from(error).into()
    }
}

impl From<sync15_adapter::Error> for Sync15PasswordsError {
    fn from(error: sync15_adapter::Error) -> Sync15PasswordsError {
        Sync15PasswordsErrorKind::from(error).into()
    }
}

impl From<serde_json::Error> for Sync15PasswordsError {
    fn from(error: serde_json::Error) -> Sync15PasswordsError {
        Sync15PasswordsErrorKind::from(error).into()
    }
}
