/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This might be where the bucketing logic can go
//! It would be different from current experimentation tools
//! There is a namespacing concept to allow users to be in multiple
//! unrelated experiments at the same time.

//! TODO: Implement the bucketing logic from the nimbus project

use serde_derive::*;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Bucket {}

impl Bucket {
    #[allow(unused)]
    pub fn new() -> Self {
        unimplemented!();
    }
}

// TODO: Implement unit testing for the bucketing logic based on the Nimbus requirments
