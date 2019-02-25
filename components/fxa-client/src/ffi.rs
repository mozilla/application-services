/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module implement the traits and some types that make the FFI code easier to manage.
//!
//! Note that the FxA FFI is older than the other FFIs in application-services, and has (direct,
//! low-level) bindings that live in the mozilla-mobile/android-components repo. As a result, it's a
//! bit harder to change (anything breaking the ABI requires careful synchronization of updates
//! across two repos), and doesn't follow all the same conventions that are followed by the other
//! FFIs.
//!
//! None of this is that bad in practice, but much of it is not ideal.

#![cfg(feature = "ffi")]

#[cfg(feature = "browserid")]
use crate::SyncKeys;
use crate::{msg_types, AccessTokenInfo, Error, ErrorKind, Profile, ScopedKey};
#[cfg(feature = "browserid")]
use ffi_support::{destroy_c_string, rust_string_to_c, IntoFfi};
use ffi_support::{
    implement_into_ffi_by_delegation, implement_into_ffi_by_protobuf, ErrorCode, ExternError,
};
#[cfg(feature = "browserid")]
use std::os::raw::c_char;

pub mod error_codes {
    // Note: -1 and 0 (panic and success) codes are reserved by the ffi-support library

    /// Catch-all error code used for anything that's not a panic or covered by AUTHENTICATION.
    pub const OTHER: i32 = 1;

    /// Used for `ErrorKind::NotMarried`, `ErrorKind::NoCachedTokens`, and `ErrorKind::RemoteError`'s
    /// where `code == 401`.
    pub const AUTHENTICATION: i32 = 2;

    /// Code for network errors.
    pub const NETWORK: i32 = 3;
}

fn get_code(err: &Error) -> ErrorCode {
    match err.kind() {
        ErrorKind::RemoteError { code: 401, .. }
        | ErrorKind::NotMarried
        | ErrorKind::NoCachedToken(_) => {
            log::warn!("Authentication error: {:?}", err);
            ErrorCode::new(error_codes::AUTHENTICATION)
        }
        ErrorKind::RequestError(_) => {
            log::warn!("Network error: {:?}", err);
            ErrorCode::new(error_codes::NETWORK)
        }
        _ => {
            log::warn!("Unexpected error: {:?}", err);
            ErrorCode::new(error_codes::OTHER)
        }
    }
}

impl From<Error> for ExternError {
    fn from(err: Error) -> ExternError {
        ExternError::new_error(get_code(&err), err.to_string())
    }
}

// `SyncKeysC` is a `#[repr(C)]` type which is heap allocated and returned
// by a boxed pointer.
//
// The fields are private for safety reasons (if they were pub, you could cause memory
// safety problems from safe rust code), but they're depended upon by the FFI, and cannot be
// changed.

#[cfg(feature = "browserid")]
#[repr(C)]
pub struct SyncKeysC {
    sync_key: *mut c_char,
    xcs: *mut c_char,
}

#[cfg(feature = "browserid")]
impl Drop for SyncKeysC {
    fn drop(&mut self) {
        unsafe {
            destroy_c_string(self.sync_key);
            destroy_c_string(self.xcs);
        }
    }
}

#[cfg(feature = "browserid")]
impl From<SyncKeys> for SyncKeysC {
    fn from(sync_keys: SyncKeys) -> Self {
        SyncKeysC {
            sync_key: rust_string_to_c(sync_keys.0),
            xcs: rust_string_to_c(sync_keys.1),
        }
    }
}

impl From<AccessTokenInfo> for msg_types::AccessTokenInfo {
    fn from(a: AccessTokenInfo) -> Self {
        msg_types::AccessTokenInfo {
            scope: a.scope,
            token: a.token,
            key: a.key.map(|k| k.into()),
            expires_at: a.expires_at,
        }
    }
}

impl From<ScopedKey> for msg_types::ScopedKey {
    fn from(sk: ScopedKey) -> Self {
        msg_types::ScopedKey {
            kty: sk.kty,
            scope: sk.scope,
            k: sk.k,
            kid: sk.kid,
        }
    }
}

impl From<Profile> for msg_types::Profile {
    fn from(p: Profile) -> Self {
        msg_types::Profile {
            avatar: Some(p.avatar),
            avatar_default: Some(p.avatar_default),
            display_name: p.display_name,
            email: Some(p.email),
            uid: Some(p.uid),
        }
    }
}

// Remove boilerplate for the conversions we need to do here. (This doesn't belong in the shared
// lib because generally I don't think this pattern is one we want to encourage. FxA is just old
// and harder to change so it's fine).
#[cfg(feature = "browserid")]
macro_rules! implement_into_ffi_converting {
    ($RootType:ident, $CType:ident) => {
        unsafe impl IntoFfi for $RootType {
            type Value = *mut $CType;

            fn ffi_default() -> Self::Value {
                ::std::ptr::null_mut()
            }

            fn into_ffi_value(self) -> Self::Value {
                Box::into_raw(Box::new($CType::from(self)))
            }
        }
    };
}

#[cfg(feature = "browserid")]
implement_into_ffi_converting!(SyncKeys, SyncKeysC);

implement_into_ffi_by_protobuf!(msg_types::Profile);
implement_into_ffi_by_delegation!(Profile, msg_types::Profile);
implement_into_ffi_by_protobuf!(msg_types::AccessTokenInfo);
implement_into_ffi_by_delegation!(AccessTokenInfo, msg_types::AccessTokenInfo);
