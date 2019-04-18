/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use nss_sys::*;
use std::{
    convert::TryFrom,
    os::raw::{c_uchar, c_uint},
    ptr,
};

// The macro defines a wrapper around pointers refering to
// types allocated by NSS and calling their NSS destructor
// method when they go out of scope avoiding memory leaks.
// The `as_ptr`/`as_mut_ptr` are provided to retrieve the
// raw pointers for the NSS functions consuming them.
macro_rules! scoped_ptr {
    ($scoped:ident, $target:ty, $dtor:path) => {
        pub struct $scoped {
            ptr: *mut $target,
        }

        impl $scoped {
            pub fn from_ptr(ptr: *mut $target) -> Result<$scoped> {
                if !ptr.is_null() {
                    Ok($scoped { ptr: ptr })
                } else {
                    Err(ErrorKind::InternalError.into())
                }
            }

            #[inline]
            #[allow(dead_code)]
            pub const fn as_ptr(&self) -> *const $target {
                self.ptr
            }

            #[inline]
            pub fn as_mut_ptr(&self) -> *mut $target {
                self.ptr
            }
        }

        impl Drop for $scoped {
            fn drop(&mut self) {
                unsafe { $dtor(self.ptr) };
            }
        }
    };
}

scoped_ptr!(Context, PK11Context, pk11_destroy_context_true);
scoped_ptr!(SymKey, PK11SymKey, PK11_FreeSymKey);
scoped_ptr!(Slot, PK11SlotInfo, PK11_FreeSlot);

#[inline]
unsafe fn pk11_destroy_context_true(context: *mut PK11Context) {
    PK11_DestroyContext(context, PR_TRUE);
}

/// Safe wrapper around `PK11_GetInternalSlot` that
/// de-allocates memory when the slot goes out of
/// scope.
pub(crate) fn get_internal_slot() -> Result<Slot> {
    Slot::from_ptr(unsafe { PK11_GetInternalSlot() })
}

/// Safe wrapper around PK11_ImportSymKey that
/// de-allocates memory when the key goes out of
/// scope.
pub(crate) fn import_sym_key(
    mechanism: CK_MECHANISM_TYPE,
    operation: CK_ATTRIBUTE_TYPE,
    buf: &[u8],
) -> Result<SymKey> {
    let mut item = SECItem {
        type_: SECItemType::siBuffer,
        data: buf.as_ptr() as *mut c_uchar,
        len: c_uint::try_from(buf.len())?,
    };
    let slot = get_internal_slot()?;
    SymKey::from_ptr(unsafe {
        PK11_ImportSymKey(
            slot.as_mut_ptr(),
            mechanism,
            PK11Origin::PK11_OriginUnwrap,
            operation,
            &mut item,
            ptr::null_mut(),
        )
    })
}

/// Safe wrapper around PK11_CreateContextBySymKey that
/// de-allocates memory when the context goes out of
/// scope.
pub(crate) fn create_context_by_sym_key(
    mechanism: CK_MECHANISM_TYPE,
    operation: CK_ATTRIBUTE_TYPE,
    sym_key: &SymKey,
) -> Result<Context> {
    let mut param = SECItem {
        type_: SECItemType::siBuffer,
        data: ptr::null_mut(),
        len: 0,
    };
    Context::from_ptr(unsafe {
        PK11_CreateContextBySymKey(mechanism, operation, sym_key.as_mut_ptr(), &mut param)
    })
}
