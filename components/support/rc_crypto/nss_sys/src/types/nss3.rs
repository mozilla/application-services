/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::nspr::*;
use std::os::raw::{c_char, c_int, c_uint};

#[repr(C)]
pub enum SECStatus {
    SECWouldBlock = -2,
    SECFailure = -1,
    SECSuccess = 0,
}
pub use SECStatus::*;

#[repr(C)]
pub enum SECOidTag {
    // We only list the values we use here.
    SEC_OID_UNKNOWN = 0,
    SEC_OID_SHA256 = 191,
}
pub use SECOidTag::*;

pub type NSSInitParameters = NSSInitParametersStr;
#[repr(C)]
pub struct NSSInitParametersStr {
    length: c_uint,
    passwordRequired: PRBool,
    minPWLen: c_int,
    manufactureID: *mut c_char,
    libraryDescription: *mut c_char,
    cryptoTokenDescription: *mut c_char,
    dbTokenDescription: *mut c_char,
    FIPSTokenDescription: *mut c_char,
    cryptoSlotDescription: *mut c_char,
    dbSlotDescription: *mut c_char,
    FIPSSlotDescription: *mut c_char,
}

pub type NSSInitContext = NSSInitContextStr;
#[repr(C)]
pub struct NSSInitContextStr {
    next: *mut NSSInitContext,
    magic: PRUint32,
}

pub const NSS_INIT_READONLY: PRUint32 = 0x1;
pub const NSS_INIT_NOCERTDB: PRUint32 = 0x2;
pub const NSS_INIT_NOMODDB: PRUint32 = 0x4;
pub const NSS_INIT_FORCEOPEN: PRUint32 = 0x8;
pub const NSS_INIT_NOROOTINIT: PRUint32 = 0x10;
pub const NSS_INIT_OPTIMIZESPACE: PRUint32 = 0x20;
pub const NSS_INIT_PK11THREADSAFE: PRUint32 = 0x40;
pub const NSS_INIT_PK11RELOAD: PRUint32 = 0x80;
pub const NSS_INIT_NOPK11FINALIZE: PRUint32 = 0x100;
pub const NSS_INIT_RESERVED: PRUint32 = 0x200;
