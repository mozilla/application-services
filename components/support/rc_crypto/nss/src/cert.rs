/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::pk11::types::{Certificate, PublicKey};
use crate::util::{assert_nss_initialized, sec_item_as_slice, ScopedPtr};
use nss_sys::{CERT_ExtractPublicKey, CERT_GetDefaultCertDB, CERT_NewTempCertificate};

pub fn extract_ec_public_key(der: &[u8]) -> Result<Vec<u8>> {
    assert_nss_initialized();

    let certdb = unsafe { CERT_GetDefaultCertDB() };
    let mut data = nss_sys::SECItem {
        len: u32::try_from(der.len())?,
        data: der.as_ptr() as *mut u8,
        type_: nss_sys::SECItemType::siBuffer as u32,
    };

    let cert = unsafe {
        Certificate::from_ptr(CERT_NewTempCertificate(
            certdb,
            &mut data,
            std::ptr::null_mut(),
            nss_sys::PR_FALSE,
            nss_sys::PR_TRUE,
        ))?
    };

    let pub_key = unsafe { PublicKey::from_ptr(CERT_ExtractPublicKey(cert.as_mut_ptr()))? };
    let pub_key_raw = unsafe { &*pub_key.as_ptr() };

    if pub_key_raw.keyType != nss_sys::KeyType::ecKey as u32 {
        return Err(
            ErrorKind::InputError("public key is not of type EC (Elliptic Curve).".into()).into(),
        );
    }

    let mut pub_key_data = unsafe { pub_key_raw.u.ec.publicValue };
    let pub_key_data_raw = unsafe { sec_item_as_slice(&mut pub_key_data)? };

    Ok(pub_key_data_raw.to_vec())
}
