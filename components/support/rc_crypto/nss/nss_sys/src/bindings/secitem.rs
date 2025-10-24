/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use crate::*;
use std::os::raw::c_uint;

extern "C" {
    pub fn SECITEM_FreeItem(zap: *mut SECItem, freeit: PRBool);
    pub fn SECITEM_AllocItem(
        arena: *mut PLArenaPool,
        item: *mut SECItem,
        len: c_uint,
    ) -> *mut SECItem;
}
