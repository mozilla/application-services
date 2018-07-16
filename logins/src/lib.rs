// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

#![recursion_limit="128"]

#![crate_name = "logins"]

extern crate chrono;
extern crate failure;
#[macro_use] extern crate failure_derive;
#[macro_use] extern crate log;
#[macro_use] extern crate lazy_static;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;

#[macro_use] extern crate mentat;

pub mod credentials;
pub mod errors;
pub use errors::{
    Error,
    Result,
};
mod json;
pub mod passwords;
pub mod types;
pub use types::{
    Credential,
    CredentialId,
    FormTarget,
    ServerPassword,
    SyncGuid,
};
mod vocab;
pub use vocab::{
    CREDENTIAL_VOCAB,
    FORM_VOCAB,
    LOGIN_VOCAB,
    SYNC_PASSWORD_VOCAB,
    ensure_vocabulary,
};

#[cfg(test)]
mod tests {
    use super::*;

    use mentat::{
        Store,
    };

    pub(crate) fn testing_store() -> Store {
        let mut store = Store::open("").expect("opened");

        // Scoped borrow of `store`.
        {
            let mut in_progress = store.begin_transaction().expect("begun successfully");

            ensure_vocabulary(&mut in_progress).expect("to ensure_vocabulary");

            in_progress.commit().expect("commit succeeded");
        }

        store
    }
}
