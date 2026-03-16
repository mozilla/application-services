/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// A dummy mozbuild which exists purely so ohttp can be built in this repo.
// This implementation is never actually called, it just needs to exist
// to keep the compiler happy.

use std::path::Path;

// from https://searchfox.org/firefox-main/source/build/rust/mozbuild/lib.rs

// Path::new is not const at the moment. This is a non-generic version
// of Path::new, similar to libstd's implementation of Path::new.
#[inline(always)]
const fn const_path(s: &'static str) -> &'static std::path::Path {
    unsafe { &*(s as *const str as *const std::path::Path) }
}

pub const TOPOBJDIR: &Path = const_path("");

pub mod config {
    pub const MOZ_FOLD_LIBS: bool = true;
    pub const BINDGEN_SYSTEM_FLAGS: [&str; 0] = [];
    pub const NSS_CFLAGS: [&str; 0] = [];
    pub const NSPR_CFLAGS: [&str; 0] = [];
}
