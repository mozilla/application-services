/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    error::*,
    pk11::{context::HashAlgorithm, slot, types::SymKey},
    util::{ensure_nss_initialized, map_nss_secstatus, sec_item_as_slice, ScopedPtr},
};
use std::{
    mem,
    os::raw::{c_uchar, c_uint, c_ulong},
    ptr,
};

pub fn hkdf_expand(
    digest_alg: &HashAlgorithm,
    key_bytes: &[u8],
    info: &[u8],
    len: usize,
) -> Result<Vec<u8>> {
    ensure_nss_initialized();
    let mech = digest_alg.as_hkdf_mechanism();
    // Most of the following code is inspired by the Firefox WebCrypto implementation:
    // https://searchfox.org/mozilla-central/rev/ee3905439acbf81e9c829ece0b46d09d2fa26c5c/dom/crypto/WebCryptoTask.cpp#2530-2597
    // Except that we only do the expand part, which explains why we use null pointers below.
    let mut hkdf_params = nss_sys::CK_NSS_HKDFParams {
        bExtract: nss_sys::CK_FALSE,
        pSalt: ptr::null_mut(),
        ulSaltLen: 0,
        bExpand: nss_sys::CK_TRUE,
        pInfo: info.as_ptr() as *mut u8,
        ulInfoLen: c_ulong::try_from(info.len())?,
    };
    let mut params = nss_sys::SECItem {
        type_: nss_sys::SECItemType::siBuffer as u32,
        data: &mut hkdf_params as *mut _ as *mut c_uchar,
        len: u32::try_from(mem::size_of::<nss_sys::CK_NSS_HKDFParams>())?,
    };
    let base_key = import_sym_key(mech.into(), nss_sys::CKA_WRAP.into(), key_bytes)?;
    let derived_len = i32::try_from(len)?;
    let sym_key = unsafe {
        SymKey::from_ptr(
            // CKM_SHA512_HMAC and CKA_SIGN are key type and usage attributes of the
            // derived symmetric key and don't matter because we ignore them anyway.
            nss_sys::PK11_Derive(
                base_key.as_mut_ptr(),
                mech.into(),
                &mut params,
                nss_sys::CKM_SHA512_HMAC.into(),
                nss_sys::CKA_SIGN.into(),
                derived_len,
            ),
        )?
    };
    map_nss_secstatus(|| unsafe { nss_sys::PK11_ExtractKeyValue(sym_key.as_mut_ptr()) })?;
    // This doesn't leak, because the SECItem* returned by PK11_GetKeyData
    // just refers to a buffer managed by `sym_key` which we copy into `out`.
    let mut key_data = unsafe { *nss_sys::PK11_GetKeyData(sym_key.as_mut_ptr()) };
    if u32::try_from(len)? > key_data.len {
        return Err(ErrorKind::InternalError.into());
    }
    let buf = unsafe { sec_item_as_slice(&mut key_data)? };
    Ok(buf.to_vec())
}

/// Safe wrapper around PK11_ImportSymKey that
/// de-allocates memory when the key goes out of
/// scope.
pub(crate) fn import_sym_key(
    mechanism: nss_sys::CK_MECHANISM_TYPE,
    operation: nss_sys::CK_ATTRIBUTE_TYPE,
    buf: &[u8],
) -> Result<SymKey> {
    ensure_nss_initialized();
    let mut item = nss_sys::SECItem {
        type_: nss_sys::SECItemType::siBuffer as u32,
        data: buf.as_ptr() as *mut c_uchar,
        len: c_uint::try_from(buf.len())?,
    };
    let slot = slot::get_internal_slot()?;
    unsafe {
        SymKey::from_ptr(nss_sys::PK11_ImportSymKey(
            slot.as_mut_ptr(),
            mechanism,
            nss_sys::PK11Origin::PK11_OriginUnwrap as u32,
            operation,
            &mut item,
            ptr::null_mut(),
        ))
    }
}

#[cfg(feature = "keydb")]
pub fn retrieve_or_create_and_import_and_persist_aes256_key_data(name: &str) -> Result<Vec<u8>> {
    let sym_key = match retrieve_aes256_key(name) {
        Ok(sym_key) => {
            map_nss_secstatus(|| unsafe { nss_sys::PK11_ExtractKeyValue(sym_key.as_mut_ptr()) })?;
            sym_key
        }
        Err(err) => {
            println!("No key found, generating one {:?}", err);
            create_and_import_and_persist_aes256_key(name).expect("could not import key")
        }
    };

    // This doesn't leak, because the SECItem* returned by PK11_GetKeyData
    // just refers to a buffer managed by `sym_key` which we copy into `out`.
    let mut key_data = unsafe { *nss_sys::PK11_GetKeyData(sym_key.as_mut_ptr()) };
    if key_data.len != nss_sys::AES_256_KEY_LENGTH {
        return Err(ErrorKind::InternalError.into());
    }
    let buf = unsafe { sec_item_as_slice(&mut key_data)? };
    Ok(buf.to_vec())
}

#[cfg(feature = "keydb")]
fn retrieve_aes256_key(name: &str) -> Result<SymKey> {
    ensure_nss_initialized();
    let slot = slot::get_internal_key_slot()?;
    let sym_key = unsafe {
        SymKey::from_ptr(nss_sys::PK11_ListFixedKeysInSlot(
            slot.as_mut_ptr(),
            name.to_string().as_ptr(),
            ptr::null_mut(),
        ))
    };
    match sym_key {
        Ok(sym_key) => {
            // See
            // https://searchfox.org/mozilla-central/source/security/manager/ssl/NSSKeyStore.cpp#163-201
            // Unfortunately we can't use PK11_ExtractKeyValue(symKey.get()) here because softoken
            // marks all token objects of type CKO_SECRET_KEY as sensitive. So we have to wrap and
            // unwrap symKey to obtain a non-sensitive copy of symKey as a session object.
            let wrapping_key = unsafe {
                SymKey::from_ptr(nss_sys::PK11_KeyGen(
                    slot.as_mut_ptr(),
                    nss_sys::CKM_AES_KEY_GEN,
                    ptr::null_mut(),
                    16,
                    ptr::null_mut(),
                ))
                .expect("could not create wrapping key")
            };
            let mut wrap_len = nss_sys::SECItem {
                type_: nss_sys::SECItemType::siBuffer as u32,
                data: ptr::null_mut(),
                len: 0,
            };
            map_nss_secstatus(|| unsafe {
                nss_sys::PK11_WrapSymKey(
                    nss_sys::CKM_AES_KEY_WRAP_KWP,
                    ptr::null_mut(),
                    wrapping_key.as_mut_ptr(),
                    sym_key.as_mut_ptr(),
                    &mut wrap_len,
                )
            })
            .expect("could not wrap key");
            let wrapped_key = unsafe {
                nss_sys::SECITEM_AllocItem(
                    ptr::null_mut(),
                    ptr::null_mut(),
                    (wrap_len.len + 8) as i32,
                )
            };
            map_nss_secstatus(|| unsafe {
                nss_sys::PK11_WrapSymKey(
                    nss_sys::CKM_AES_KEY_WRAP_KWP,
                    ptr::null_mut(),
                    wrapping_key.as_mut_ptr(),
                    sym_key.as_mut_ptr(),
                    wrapped_key,
                )
            })
            .expect("could not wrap key again");
            unsafe {
                SymKey::from_ptr(nss_sys::PK11_UnwrapSymKey(
                    wrapping_key.as_mut_ptr(),
                    nss_sys::CKM_AES_KEY_WRAP_KWP,
                    ptr::null_mut(),
                    wrapped_key,
                    nss_sys::CKM_AES_GCM.into(),
                    (nss_sys::CKA_ENCRYPT | nss_sys::CKA_DECRYPT).into(),
                    wrap_len.len as i32,
                ))
            }
        }
        Err(e) => Err(e),
    }
}

#[cfg(feature = "keydb")]
fn create_and_import_and_persist_aes256_key(name: &str) -> Result<SymKey> {
    ensure_nss_initialized();
    let mut key_bytes: [u8; nss_sys::AES_256_KEY_LENGTH as usize] =
        [0; nss_sys::AES_256_KEY_LENGTH as usize];
    map_nss_secstatus(|| unsafe {
        nss_sys::PK11_GenerateRandom(key_bytes.as_mut_ptr(), nss_sys::AES_256_KEY_LENGTH as i32)
    })?;
    match import_and_persist_sym_key(
        nss_sys::CKM_AES_GCM.into(),
        nss_sys::PK11Origin::PK11_OriginGenerated,
        (nss_sys::CKA_ENCRYPT | nss_sys::CKA_DECRYPT).into(),
        &key_bytes,
    ) {
        Ok(sym_key) => {
            unsafe {
                nss_sys::PK11_SetSymKeyNickname(sym_key.as_mut_ptr(), name.to_string().as_ptr())
            };
            Ok(sym_key)
        }
        Err(e) => Err(e),
    }
}

/// Safe wrapper around PK11_ImportSymKey that
/// de-allocates memory when the key goes out of
/// scope, and persists key in key4.db.
#[cfg(feature = "keydb")]
fn import_and_persist_sym_key(
    mechanism: nss_sys::CK_MECHANISM_TYPE,
    origin: nss_sys::PK11Origin,
    operation: nss_sys::CK_ATTRIBUTE_TYPE,
    buf: &[u8],
) -> Result<SymKey> {
    ensure_nss_initialized();
    let mut item = nss_sys::SECItem {
        type_: nss_sys::SECItemType::siBuffer as u32,
        data: buf.as_ptr() as *mut c_uchar,
        len: c_uint::try_from(buf.len())?,
    };
    let slot = slot::get_internal_key_slot()?;
    unsafe {
        // FIXME: this does not work until we set isPerm to false
        SymKey::from_ptr(nss_sys::PK11_ImportSymKeyWithFlags(
            slot.as_mut_ptr(),
            mechanism,
            origin as u32,
            operation,
            &mut item,
            nss_sys::CK_FLAGS::default(),
            nss_sys::PR_TRUE,
            ptr::null_mut(),
        ))
    }
}
