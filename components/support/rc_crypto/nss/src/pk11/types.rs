/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    error::*,
    pk11::slot::get_internal_slot,
    util::{map_nss_secstatus, ScopedPtr},
};
use std::{
    ops::Deref,
    os::raw::{c_uint, c_void},
    ptr,
};

scoped_ptr!(SymKey, nss_sys::PK11SymKey, nss_sys::PK11_FreeSymKey);
scoped_ptr!(
    PrivateKey,
    nss_sys::SECKEYPrivateKey,
    nss_sys::SECKEY_DestroyPrivateKey
);
scoped_ptr!(
    PublicKey,
    nss_sys::SECKEYPublicKey,
    nss_sys::SECKEY_DestroyPublicKey
);
scoped_ptr!(
    Certificate,
    nss_sys::CERTCertificate,
    nss_sys::CERT_DestroyCertificate
);

scoped_ptr!(Context, nss_sys::PK11Context, pk11_destroy_context_true);
scoped_ptr!(Slot, nss_sys::PK11SlotInfo, nss_sys::PK11_FreeSlot);

scoped_ptr!(
    AlgorithmID,
    nss_sys::SECAlgorithmID,
    secoid_destroy_algorithm_id_true
);

#[inline]
unsafe fn secoid_destroy_algorithm_id_true(alg_id: *mut nss_sys::SECAlgorithmID) {
    nss_sys::SECOID_DestroyAlgorithmID(alg_id, nss_sys::PR_TRUE);
}

#[inline]
unsafe fn pk11_destroy_context_true(context: *mut nss_sys::PK11Context) {
    nss_sys::PK11_DestroyContext(context, nss_sys::PR_TRUE);
}

// Trait for types that have PCKS#11 attributes that are readable. See
// https://searchfox.org/mozilla-central/rev/8ed8474757695cdae047150a0eaf94a5f1c96dbe/security/nss/lib/pk11wrap/pk11pub.h#842-864
/// # Safety
/// Unsafe since it needs to call [`nss_sys::PK11_ReadRawAttribute`] which is
/// a C NSS function, and thus inherently unsafe to call
pub(crate) unsafe trait Pkcs11Object: ScopedPtr {
    const PK11_OBJECT_TYPE: nss_sys::PK11ObjectType;
    fn read_raw_attribute(
        &self,
        attribute_type: nss_sys::CK_ATTRIBUTE_TYPE,
    ) -> Result<ScopedSECItem> {
        let mut out_sec = ScopedSECItem::empty(nss_sys::SECItemType::siBuffer);
        map_nss_secstatus(|| unsafe {
            nss_sys::PK11_ReadRawAttribute(
                Self::PK11_OBJECT_TYPE as u32,
                self.as_mut_ptr() as *mut c_void,
                attribute_type,
                out_sec.as_mut_ref(),
            )
        })?;
        Ok(out_sec)
    }
}

unsafe impl Pkcs11Object for PrivateKey {
    const PK11_OBJECT_TYPE: nss_sys::PK11ObjectType = nss_sys::PK11ObjectType::PK11_TypePrivKey;
}
unsafe impl Pkcs11Object for PublicKey {
    const PK11_OBJECT_TYPE: nss_sys::PK11ObjectType = nss_sys::PK11ObjectType::PK11_TypePubKey;
}
unsafe impl Pkcs11Object for SymKey {
    const PK11_OBJECT_TYPE: nss_sys::PK11ObjectType = nss_sys::PK11ObjectType::PK11_TypeSymKey;
}

// From https://developer.mozilla.org/en-US/docs/Mozilla/Projects/NSS/NSS_API_Guidelines#Thread_Safety:
// "Data structures that are read only, like SECKEYPublicKeys or PK11SymKeys, need not be protected."
unsafe impl Send for PrivateKey {}
unsafe impl Send for PublicKey {}

impl PrivateKey {
    pub fn convert_to_public_key(&self) -> Result<PublicKey> {
        Ok(unsafe { PublicKey::from_ptr(nss_sys::SECKEY_ConvertToPublicKey(self.as_mut_ptr()))? })
    }

    pub(crate) fn from_private_key_template(template: Vec<nss_sys::CK_ATTRIBUTE>) -> Result<Self> {
        let slot = get_internal_slot()?;
        let count = c_uint::try_from(template.len())?;
        Ok(unsafe {
            PrivateKey::from_ptr(nss_sys::PK11_CreatePrivateKeyFromTemplate(
                slot.as_mut_ptr(),
                template.as_ptr(),
                count,
                ptr::null_mut(),
            ))?
        })
    }
}

// This is typically used by functions receiving a pointer to an `out SECItem`,
// where we allocate the struct, but NSS allocates the elements it points to.
pub(crate) struct ScopedSECItem {
    wrapped: nss_sys::SECItem,
}

impl ScopedSECItem {
    pub(crate) fn empty(r#type: nss_sys::SECItemType) -> Self {
        ScopedSECItem {
            wrapped: nss_sys::SECItem {
                type_: r#type as u32,
                data: ptr::null_mut(),
                len: 0,
            },
        }
    }

    pub(crate) fn as_mut_ref(&mut self) -> &mut nss_sys::SECItem {
        &mut self.wrapped
    }
}

impl Deref for ScopedSECItem {
    type Target = nss_sys::SECItem;
    #[inline]
    fn deref(&self) -> &nss_sys::SECItem {
        &self.wrapped
    }
}

impl Drop for ScopedSECItem {
    fn drop(&mut self) {
        unsafe {
            // PR_FALSE asks the NSS allocator not to free the SECItem
            // itself, and just the pointee of `self.wrapped.data`.
            nss_sys::SECITEM_FreeItem(&mut self.wrapped, nss_sys::PR_FALSE);
        }
    }
}
