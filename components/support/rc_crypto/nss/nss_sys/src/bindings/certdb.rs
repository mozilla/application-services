/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::*;
use std::os::raw::c_char;

// Opaque types
pub type CERTCertDBHandle = u8;
pub type CERTCertificate = u8;

extern "C" {
    pub fn CERT_GetDefaultCertDB() -> *mut CERTCertDBHandle;

    pub fn CERT_NewTempCertificate(
        handle: *mut CERTCertDBHandle,
        derCert: *mut SECItem,
        nickname: *mut c_char,
        isperm: PRBool,
        copyDER: PRBool,
    ) -> *mut CERTCertificate;

    pub fn CERT_DestroyCertificate(cert: *mut CERTCertificate);
}
