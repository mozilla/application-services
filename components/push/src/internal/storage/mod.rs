/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Handles Push component storage
//!
//! Mainly exposes a trait, [`Storage`] and a concrete type that implements the trait, [`Store`]
//!
//! [`Storage`] is a trait representing the storage of records. Each record is a subscription record associated with a `channel_id`
//!
//! Records mainly include the autopush endpoint senders need to send their payloads to and the private key associated with the subscription
//! The records act as both:
//! - A cache for subscription records that are returned when senders re-subscribe to an already subscribed channel
//! - Storage for the private keys used to decrypt push payloads

mod db;
mod record;
mod schema;

pub use self::{
    db::{PushDb as Store, Storage},
    record::PushRecord,
};
