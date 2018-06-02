/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use fxa_client::http_client::ProfileResponse;
use fxa_client::{OAuthInfo, SyncKeys};
use libc::c_char;
use std;
use util::*;

#[repr(C)]
pub struct SyncKeysC {
    pub sync_key: *mut c_char,
    pub xcs: *mut c_char,
}

impl From<SyncKeys> for SyncKeysC {
    fn from(sync_keys: SyncKeys) -> Self {
        SyncKeysC {
            sync_key: string_to_c_char(sync_keys.0),
            xcs: string_to_c_char(sync_keys.1),
        }
    }
}

#[repr(C)]
pub struct OAuthInfoC {
    pub access_token: *mut c_char,
    pub keys_jwe: *mut c_char,
    pub scope: *mut c_char,
}

impl From<OAuthInfo> for OAuthInfoC {
    fn from(info: OAuthInfo) -> Self {
        let scopes = info.scopes.join(" ");
        OAuthInfoC {
            access_token: string_to_c_char(info.access_token),
            keys_jwe: match info.keys_jwe {
                Some(keys_jwe) => string_to_c_char(keys_jwe),
                None => std::ptr::null_mut(),
            },
            scope: string_to_c_char(scopes),
        }
    }
}

#[repr(C)]
pub struct ProfileC {
    pub uid: *mut c_char,
    pub email: *mut c_char,
    pub avatar: *mut c_char,
}

impl From<ProfileResponse> for ProfileC {
    fn from(profile: ProfileResponse) -> Self {
        ProfileC {
            uid: string_to_c_char(profile.uid),
            email: string_to_c_char(profile.email),
            avatar: string_to_c_char(profile.avatar),
        }
    }
}
