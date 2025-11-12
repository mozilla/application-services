/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(feature = "keydb")]
use crate::pk11::types::Slot;
#[cfg(feature = "keydb")]
use crate::util::get_last_error;
use crate::{
    error::*,
    pk11::{context::HashAlgorithm, slot, types::SymKey},
    util::{assert_nss_initialized, map_nss_secstatus, sec_item_as_slice, ScopedPtr},
};
#[cfg(feature = "keydb")]
use std::ffi::{c_char, CString};
#[cfg(feature = "keydb")]
use std::sync::{Mutex, OnceLock};
use std::{
    mem,
    os::raw::{c_uchar, c_uint, c_ulong},
    ptr,
};

// Serialize global state dependent function calls
#[cfg(feature = "keydb")]
static GLOBAL_TOKEN_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn hkdf_expand(
    digest_alg: &HashAlgorithm,
    key_bytes: &[u8],
    info: &[u8],
    len: usize,
) -> Result<Vec<u8>> {
    assert_nss_initialized();
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
    // SAFETY: This doesn't leak, because the SECItem* returned by PK11_GetKeyData just refers to a
    // buffer managed by `sym_key` which we copy into `out`.
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
    assert_nss_initialized();
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

/// Check weather a primary password has been set and NSS needs to be authenticated.
/// Only available with the `keydb` feature.
#[cfg(feature = "keydb")]
pub fn authentication_with_primary_password_is_needed() -> Result<bool> {
    // PK11_IsLoggedIn depends on the token state
    let lock = GLOBAL_TOKEN_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();

    let slot = slot::get_internal_key_slot()?;

    unsafe {
        Ok(
            nss_sys::PK11_NeedLogin(slot.as_mut_ptr()) == nss_sys::PR_TRUE
                && nss_sys::PK11_IsLoggedIn(slot.as_mut_ptr(), ptr::null_mut()) != nss_sys::PR_TRUE,
        )
    }
}

/// Authorize NSS key store against a user-provided primary password.
/// Only available with the `keydb` feature.
#[cfg(feature = "keydb")]
pub fn authenticate_with_primary_password(primary_password: &str) -> Result<bool> {
    // this needs serializing because PK11_CheckUserPassword first loggs the user out
    let lock = GLOBAL_TOKEN_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();

    let slot = slot::get_internal_key_slot()?;

    let password_cstr = CString::new(primary_password).map_err(|_| ErrorKind::NulError)?;
    unsafe {
        Ok(
            nss_sys::PK11_CheckUserPassword(slot.as_mut_ptr(), password_cstr.as_ptr())
                == nss_sys::SECStatus::SECSuccess,
        )
    }
}

/// Retrieve a key, identified by `name`, from the internal NSS key store. If none exists, create
/// one, persist, and return.
/// Only available with the `keydb` feature.
#[cfg(feature = "keydb")]
pub fn get_or_create_aes256_key(name: &str) -> Result<Vec<u8>> {
    let sym_key = match get_aes256_key(name)? {
        Some(sym_key) => sym_key,
        None => create_aes256_key(name)?,
    };
    let mut key_data = unsafe { *nss_sys::PK11_GetKeyData(sym_key.as_mut_ptr()) };
    if key_data.len != nss_sys::AES_256_KEY_LENGTH {
        return Err(ErrorKind::InvalidKeyLength.into());
    }
    let buf = unsafe { sec_item_as_slice(&mut key_data)? };
    // SAFETY: `to_vec` copies the data out before there's any chance for `sym_key` to be
    // destroyed.
    Ok(buf.to_vec())
}

#[cfg(feature = "keydb")]
pub fn get_aes256_key(name: &str) -> Result<Option<SymKey>> {
    // PK11_ListFixedKeysInSlot depends on the token to be unlocked
    let lock = GLOBAL_TOKEN_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();

    let slot = slot::get_internal_key_slot()?;
    let c_name = CString::new(name).map_err(|_| ErrorKind::NulError)?;
    let sym_key = unsafe {
        // PK11_ListFixedKeysInSlot returns either a key or null.
        SymKey::from_ptr(nss_sys::PK11_ListFixedKeysInSlot(
            slot.as_mut_ptr(),
            c_name.as_ptr() as *mut c_char,
            ptr::null_mut(),
        ))
    };
    match sym_key {
        Ok(sym_key) => match extract_aes256_key_value(slot, sym_key) {
            Ok(key) => Ok(Some(key)),
            Err(e) => Err(e),
        },
        Err(_) => Ok(None),
    }
}

#[cfg(feature = "keydb")]
// See
// https://searchfox.org/mozilla-central/source/security/manager/ssl/NSSKeyStore.cpp#163-201
//
// Unfortunately we can't use PK11_ExtractKeyValue(symKey.get()) here because softoken
// marks all token objects of type CKO_SECRET_KEY as sensitive. So we have to wrap and
// unwrap symKey to obtain a non-sensitive copy of symKey as a session object.
fn extract_aes256_key_value(slot: Slot, sym_key: SymKey) -> Result<SymKey> {
    let wrapping_key = unsafe {
        SymKey::from_ptr(nss_sys::PK11_KeyGen(
            slot.as_mut_ptr(),
            nss_sys::CKM_AES_KEY_GEN,
            ptr::null_mut(),
            16,
            ptr::null_mut(),
        ))
        .map_err(|_| get_last_error())?
    };

    // Allocate an extra 8 bytes for CKM_AES_KEY_WRAP_KWP overhead.
    let mut buf = vec![
        0;
        (nss_sys::AES_256_KEY_LENGTH + 8)
            .try_into()
            .expect("invalid key length")
    ];
    let mut wrapped_key = nss_sys::SECItem {
        type_: nss_sys::SECItemType::siBuffer as u32,
        data: buf.as_mut_ptr(),
        len: buf.len() as u32,
    };

    // This operation can fail if the sym_key is corrupt.
    // If thats the case, the underlying NSC_WrapKey would return a
    // CKR_KEY_TYPE_INCONSISTENT error, which won't get propagated to us here,
    // though.
    map_nss_secstatus(|| unsafe {
        nss_sys::PK11_WrapSymKey(
            nss_sys::CKM_AES_KEY_WRAP_KWP,
            ptr::null_mut(),
            wrapping_key.as_mut_ptr(),
            sym_key.as_mut_ptr(),
            &mut wrapped_key,
        )
    })
    .map_err(|_| get_last_error())?;

    let sym_key = unsafe {
        SymKey::from_ptr(nss_sys::PK11_UnwrapSymKey(
            wrapping_key.as_mut_ptr(),
            nss_sys::CKM_AES_KEY_WRAP_KWP,
            ptr::null_mut(),
            &mut wrapped_key,
            nss_sys::CKM_AES_GCM.into(),
            (nss_sys::CKA_ENCRYPT | nss_sys::CKA_DECRYPT).into(),
            nss_sys::AES_256_KEY_LENGTH as i32,
        ))
    }
    .map_err(|_| get_last_error())?;

    map_nss_secstatus(|| unsafe { nss_sys::PK11_ExtractKeyValue(sym_key.as_mut_ptr()) })?;

    Ok(sym_key)
}

#[cfg(feature = "keydb")]
fn create_aes256_key(name: &str) -> Result<SymKey> {
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
            let name = CString::new(name).map_err(|_| ErrorKind::NulError)?;
            unsafe { nss_sys::PK11_SetSymKeyNickname(sym_key.as_mut_ptr(), name.as_ptr()) };
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
    // PK11_ImportSymKeyWithFlags depends on the token to be unlocked in order
    // to encrypt the key
    let lock = GLOBAL_TOKEN_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();

    let mut item = nss_sys::SECItem {
        type_: nss_sys::SECItemType::siBuffer as u32,
        data: buf.as_ptr() as *mut c_uchar,
        len: c_uint::try_from(buf.len())?,
    };
    let slot = slot::get_internal_key_slot()?;
    unsafe {
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

#[cfg(feature = "keydb")]
#[cfg(test)]
mod keydb_test {
    use super::*;
    use crate::ensure_initialized_with_profile_dir;
    use std::path::PathBuf;
    use std::thread;

    fn profile_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/profile")
    }

    #[test]
    fn test_get_aes256_key_not_found() {
        ensure_initialized_with_profile_dir(profile_path());
        authenticate_with_primary_password("password").unwrap();
        let result = get_aes256_key("unknown-key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_aes256_key() {
        ensure_initialized_with_profile_dir(profile_path());
        authenticate_with_primary_password("password").unwrap();
        let result = get_aes256_key("as-logins-key").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_get_aes256_key_parallel() {
        ensure_initialized_with_profile_dir(profile_path());

        let threads: Vec<_> = (0..100)
            .map(|_| {
                thread::spawn(move || {
                    let authenticated = authenticate_with_primary_password("password").unwrap();
                    assert!(authenticated);
                    let result = get_aes256_key("as-logins-key").unwrap();
                    assert!(result.is_some());
                })
            })
            .collect();

        for handle in threads {
            handle.join().unwrap();
        }
    }
}
