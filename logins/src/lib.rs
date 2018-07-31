// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

//! This crate is an interface for working with Sync 1.5 passwords and arbitrary logins.
//!
//! We use "passwords" or "password records" to talk about Sync 1.5's object format stored in the
//! "passwords" collection.  We use "logins" to talk about local credentials, which will grow to be
//! more general than Sync 1.5's limited object format.
//!
//! For Sync 1.5 passwords, we reference the somewhat out-dated but still useful [client
//! documentation](https://mozilla-services.readthedocs.io/en/latest/sync/objectformats.html#passwords).
//!
//! # Data model
//!
//! There are three fundamental parts to the model of logins implemented:
//! 1. *credentials* are username/password pairs
//! 1. *forms* are contexts where credentials can be used
//! 1. *logins* are usages: this *credential* was used to login to this *form*
//!
//! In this model, a user might have a single username/password pair for their Google Account;
//! enter it into multiple forms (say, login forms on "mail.google.com" and "calendar.google.com",
//! and a password reset form on "accounts.google.com"); and have used the login forms weekly but
//! the password reset form only once.
//!
//! This model can grow to accommodate new types of credentials and new contexts for usage.  A new
//! credential might be a hardware key (like Yubikey) that is identified by a device serial number;
//! or it might be a cookie from a web browser login.  And a password manager might be on a mobile
//! device and not embedded in a Web browser: it might provide credentials to specific Apps as a
//! platform-specific password filling API.  In this case, the context is not a *form*.
//!
//! To support Sync 1.5, we add a fourth fundamental part to the model: a Sync password notion that
//! glues together a credential, a form, and some materialized logins usage data.  The
//! [`ServerPassword`] type captures these notions.
//!
//! # Limitations of the Sync 1.5 object model
//!
//! There are many limitations of the Sync 1.5 object model, but the two most significant for this
//! implementation are:
//!
//! 1. A consumer that is *not a Web browser* can't smoothly create Sync 1.5 password records!
//! Consider the password manager on a mobile device not embedded in a Web browser: there is no way
//! for it to associate login usage with a particular web site, let alone a particular form.  That
//! is, the only usage context that Sync 1.5 password records accommodates looks exactly like
//! Firefox's usage context.  (Any consumer can fabricate required entries in the `ServerPassword`
//! type, or require the user to provide them -- but the product experience will suffer.)
//!
//! 1. It can't represent the use of the same username/password pair across more than one site,
//! leading to the creation of add-ons like
//! [mass-password-reset](https://addons.mozilla.org/en-US/firefox/addon/mass-password-reset/). There
//! is a many-to-many relationship between credentials and forms.  Firefox Desktop and Firefox Sync
//! both duplicate credentials when they're saved after use in multiple places.  But conversely,
//! note that there are situations in which the same username and password mean different things:
//! the most common is password reuse.

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
