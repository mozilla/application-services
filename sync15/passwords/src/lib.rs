// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

#![crate_name = "sync15_passwords"]

extern crate failure;
#[macro_use] extern crate failure_derive;
#[macro_use] extern crate log;
extern crate serde_json;

extern crate mentat;

extern crate logins;
extern crate sync15_adapter;

pub mod engine;
pub use engine::{
    PasswordEngine,
};
pub mod errors;
pub use errors::{
    Sync15PasswordsError,
    Sync15PasswordsErrorKind,
    Result,
};

pub use logins::{
    ServerPassword,
    credentials,
    passwords,
};
