/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(non_camel_case_types)]

pub type PRBool = PRIntn;
pub const PR_TRUE: PRBool = 1;
pub const PR_FALSE: PRBool = 0;
pub type PRErrorCode = PRInt32;
pub type PRInt32 = i32;
pub type PRUint32 = u32;
pub type PRIntn = std::os::raw::c_int;
pub type PRUintn = std::os::raw::c_uint;

#[repr(C)]
pub enum PRThreadPriority {
    PR_PRIORITY_LOW = 0,
    PR_PRIORITY_NORMAL = 1,
    PR_PRIORITY_HIGH = 2,
    PR_PRIORITY_URGENT = 3,
}

#[repr(C)]
pub enum PRThreadType {
    PR_USER_THREAD,
    PR_SYSTEM_THREAD,
}
