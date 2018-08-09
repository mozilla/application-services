/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use fxa_client::Profile;
use fxa_client::{OAuthInfo, SyncKeys};
use fxa_str_free;
use libc::c_char;
use std;
use util::*;

#[repr(C)]
pub struct SyncKeysC {
    pub sync_key: *mut c_char,
    pub xcs: *mut c_char,
}

impl Drop for SyncKeysC {
    fn drop(&mut self) {
        fxa_str_free(self.sync_key);
        fxa_str_free(self.xcs);
    }
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
    pub keys: *mut c_char,
    pub scope: *mut c_char,
}

impl Drop for OAuthInfoC {
    fn drop(&mut self) {
        fxa_str_free(self.access_token);
        fxa_str_free(self.keys);
        fxa_str_free(self.scope);
    }
}

impl From<OAuthInfo> for OAuthInfoC {
    fn from(info: OAuthInfo) -> Self {
        let scopes = info.scopes.join(" ");
        OAuthInfoC {
            access_token: string_to_c_char(info.access_token),
            keys: match info.keys {
                Some(keys) => string_to_c_char(keys),
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
    pub display_name: *mut c_char,
}

impl Drop for ProfileC {
    fn drop(&mut self) {
        fxa_str_free(self.uid);
        fxa_str_free(self.email);
        fxa_str_free(self.avatar);
        fxa_str_free(self.display_name);
    }
}

impl From<Profile> for ProfileC {
    fn from(profile: Profile) -> Self {
        ProfileC {
            uid: string_to_c_char(profile.uid),
            email: string_to_c_char(profile.email),
            avatar: string_to_c_char(profile.avatar),
            display_name: profile
                .display_name
                .map_or(std::ptr::null_mut(), |s| string_to_c_char(s)),
        }
    }
}
