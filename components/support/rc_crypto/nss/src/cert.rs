/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::util::{ensure_nss_initialized, sec_item_as_slice};
// use crate::error::ErrorKind;
use nss_sys::CERT_ExtractPublicKey;
use nss_sys::{CERT_GetDefaultCertDB, CERT_NewTempCertificate};
use std::convert::TryFrom;

pub fn extract_public_key(der: &[u8]) -> Result<Vec<u8>> {
    ensure_nss_initialized();

    let certdb = unsafe { CERT_GetDefaultCertDB() };
    let mut data = nss_sys::SECItem {
        len: u32::try_from(der.len())?,
        data: der.as_ptr() as *mut u8,
        type_: nss_sys::SECItemType::siDERCertBuffer as u32,
    };

    let cert = unsafe {
        CERT_NewTempCertificate(
            certdb,
            &mut data,
            std::ptr::null_mut(),
            nss_sys::PR_FALSE,
            nss_sys::PR_TRUE,
        )
    };

    let pub_key = unsafe { CERT_ExtractPublicKey(cert) };
    let mut pub_key_data = unsafe { (*pub_key).u.ec.publicValue };
    let pub_key_data_raw = unsafe { sec_item_as_slice(&mut pub_key_data)? };

    // TODO: Free resources!

    Ok(pub_key_data_raw.to_vec())
}
