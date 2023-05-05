/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::ffi::CString;
use std::os::raw::c_char;

pub use nimbus as cirrus;
pub use nimbus_fml as fml;

#[no_mangle]
pub extern "C" fn cirrus_megazord_get_version() -> *const c_char {
    VERSION_PTR.0
}

// This is set by gradle, but wouldn't be set otherwise. If it is unset,
// we'll return null from this function, which will cause the megazord
// version checker to throw. Separated as a constant to make it clear that
// this is a thing determined at compile time.
static VERSION: Option<&str> = option_env!("MEGAZORD_VERSION");

// For now it's tricky for this string to get freed, so just allocate one and save its pointer.
lazy_static::lazy_static! {
    static ref VERSION_PTR: StaticCStringPtr = StaticCStringPtr(
        VERSION.and_then(|s| CString::new(s).ok())
            .map_or(std::ptr::null(), |cs| cs.into_raw()));
}

// Wrapper that lets us keep a raw pointer in a lazy_static
#[repr(transparent)]
#[derive(Copy, Clone)]
struct StaticCStringPtr(*const c_char);
unsafe impl Send for StaticCStringPtr {}
unsafe impl Sync for StaticCStringPtr {}
