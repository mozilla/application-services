/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(feature = "backend-c")]
mod c;
#[cfg(feature = "backend-reqwest")]
mod reqwest;

#[cfg(feature = "backend-reqwest")]
pub use self::reqwest::*;
#[cfg(feature = "backend-c")]
pub use c::*;
