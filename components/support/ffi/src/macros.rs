/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde;
use serde_json;
use std::os::raw::c_char;
use string::rust_string_to_c;

/// Implements [`IntoFfi`] for the provided types (more than one may be passed in) by allocating
/// `$T` on the heap as an opaque pointer.
///
/// This is typically going to be used from the "Rust component", and not the "FFI component" (see
/// the top level crate documentation for more information), however you will still need to
/// implement a destructor in the FFI component using [`define_box_destructor!`].
#[macro_export]
macro_rules! implement_into_ffi_by_pointer {
    ($($T:ty),* $(,)*) => {$(
        unsafe impl $crate::IntoFfi for $T {
            type Value = *mut $T;

            #[inline]
            fn ffi_default() -> *mut $T {
                ::std::ptr::null_mut()
            }

            #[inline]
            fn into_ffi_value(self) -> *mut $T {
                ::std::boxed::Box::into_raw(::std::boxed::Box::new(self))
            }
        }
    )*}
}

/// Implements [`IntoFfi`] for the provided types (more than one may be passed in) by converting to
/// the type to a JSON string. This macro also allows you to return `Vec<T>` for the types, also by
/// serialization to JSON (by way of [`IntoFfiJsonTag`]).
///
/// This is typically going to be used from the "Rust component", and not the "FFI component" (see
/// the top level crate documentation for more information).
///
/// Note: Each type passed in must implement or derive `serde::Serialize`.
///
/// ## Panics
///
/// The [`IntoFfi`] implementation this macro generates may panic in the following cases:
///
/// - You've passed a type that contains a Map that has non-string keys (which can't be represented
///   in JSON).
///
/// - You've passed a type which has a custom serializer, and the custom serializer failed.
///
/// These cases are both rare enough that this still seems fine for the majority of uses.
#[macro_export]
macro_rules! implement_into_ffi_by_json {
    ($($T:ty),* $(,)*) => {$(
        unsafe impl $crate::IntoFfi for $T {
            type Value = *mut ::std::os::raw::c_char;
            #[inline]
            fn ffi_default() -> *mut ::std::os::raw::c_char {
                ::std::ptr::null_mut()
            }
            #[inline]
            fn into_ffi_value(self) -> *mut ::std::os::raw::c_char {
                $crate::convert_to_json_string(&self)
            }
        }

        impl $crate::IntoFfiJsonTag for $T {}
    )*}
}

/// For a number of reasons (name collisions are a big one, but, it also wouldn't work on all
/// platforms), we cannot export `extern "C"` functions from this library. However, it's pretty
/// common to want to free strings allocated by rust, so many libraries will need this, so we
/// provide it as a macro.
///
/// It simply expands to a `#[no_mangle] pub unsafe extern "C" fn` which wraps this crate's
/// [`destroy_c_string`] function.
///
/// ## Caveats
///
/// If you're using multiple separately compiled rust libraries in your application, it's critical
/// that you are careful to only ever free strings allocated by a Rust library using the same rust
/// library. Passing them to a different Rust library's string destructor will cause you to corrupt
/// multiple heaps.
///
/// Additionally, be sure that all strings you pass to this were actually allocated by rust. It's a
/// common issue for JNA code to transparently convert Pointers to things to Strings behind the
/// scenes, which is quite risky here. (To avoid this in JNA, only use `String` for passing
/// read-only strings into Rust, e.g. it's for passing `*const c_char`. All other uses should use
/// `Pointer` and `getString()`).
///
/// Finally, to avoid name collisions, it is strongly recommended that you provide an name for this
/// function unique to your library.
///
/// ## Example
///
/// ```rust
/// # #[macro_use] extern crate ffi_support;
/// define_string_destructor!(mylib_destroy_string);
/// ```
#[macro_export]
macro_rules! define_string_destructor {
    ($mylib_destroy_string:ident) => {
        #[doc = "Public destructor for strings managed by the other side of the FFI."]
        #[no_mangle]
        pub unsafe extern "C" fn $mylib_destroy_string(s: *mut ::std::os::raw::c_char) {
            if !s.is_null() {
                $crate::destroy_c_string(s)
            }
        }
    };
}

/// Define a (public) destructor for a type that was allocated by `Box::into_raw(Box::new(value))`
/// (e.g. a pointer which is probably opaque).
///
/// ## Caveats
///
/// This can go wrong in a ridiculous number of ways, and we can't really prevent any of them. But
/// essentially, the caller (on the other side of the FFI) needs to be extremely careful to ensure
/// that it stops using the pointer after it's freed.
///
/// Also, to avoid name collisions, it is strongly recommended that you provide an name for this
/// function unique to your library. (This is true for all functions you expose).
///
/// ## Example
///
/// ```rust
/// # #[macro_use] extern crate ffi_support;
/// struct CoolType(Vec<i32>);
///
/// define_box_destructor!(CoolType, mylib_destroy_cooltype);
/// ```
#[macro_export]
macro_rules! define_box_destructor {
    ($T:ty, $destructor_name:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $destructor_name(v: *mut $T) {
            if !v.is_null() {
                drop(::std::boxed::Box::from_raw(v))
            }
        }
    };
}

// Needs to be pub so the macro can call it, but that's all.
#[doc(hidden)]
pub fn convert_to_json_string<T: serde::Serialize>(value: &T) -> *mut c_char {
    // This panic is inside our catch_panic, so it should be fine. We've also documented the case
    // where the IntoFfi impl that calls this panics, and it's rare enough that it shouldn't matter
    // that if it happens we return an ExternError representing a panic instead of one of some other
    // type (especially given that the application isn't likely to be able to meaningfully handle
    // JSON serialization failure).
    let as_string = serde_json::to_string(&value).unwrap();
    rust_string_to_c(as_string)
}
