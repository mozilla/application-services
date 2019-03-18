/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::string::rust_string_to_c;
use std::os::raw::c_char;

/// Implements [`IntoFfi`] for the provided types (more than one may be passed in) by allocating
/// `$T` on the heap as an opaque pointer.
///
/// This is typically going to be used from the "Rust component", and not the "FFI component" (see
/// the top level crate documentation for more information), however you will still need to
/// implement a destructor in the FFI component using [`define_box_destructor!`].
///
/// In general, is only safe to do for `send` types (even this is dodgy, but it's often necessary
/// to keep the locking on the other side of the FFI, so Sync is too harsh), so we enforce this in
/// this macro. (You're still free to implement this manually, if this restriction is too harsh
/// for your use case and you're certain you know what you're doing).
#[macro_export]
macro_rules! implement_into_ffi_by_pointer {
    ($($T:ty),* $(,)*) => {$(
        unsafe impl $crate::IntoFfi for $T where $T: Send {
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
    )*}
}

/// Implements [`IntoFfi`] for the provided types (more than one may be passed in) implementing
/// `prost::Message` (protobuf auto-generated type) by converting to the type to a [`ByteBuffer`].
/// This [`ByteBuffer`] should later be passed by value.
///
/// Note: Each type passed in must implement or derive `prost::Message`.
#[cfg(feature = "prost_support")]
#[macro_export]
macro_rules! implement_into_ffi_by_protobuf {
    ($($FFIType:ty),* $(,)*) => {$(
        unsafe impl $crate::IntoFfi for $FFIType where $FFIType: prost::Message {
            type Value = $crate::ByteBuffer;
            #[inline]
            fn ffi_default() -> Self::Value {
                Default::default()
            }

            #[inline]
            fn into_ffi_value(self) -> Self::Value {
                use prost::Message;
                let mut bytes = Vec::with_capacity(self.encoded_len());
                // Unwrap is safe, since we have reserved sufficient capacity in
                // the vector.
                self.encode(&mut bytes).unwrap();
                bytes.into()
            }
        }
    )*}
}

/// Implement IntoFfi for a type by converting through another type.
///
/// The argument `$MidTy` argument must implement `From<$SrcTy>` and
/// [`InfoFfi`].
///
/// This is provided (even though it's trivial) because it is always safe (well,
/// so long as `$MidTy`'s [`IntoFfi`] implementation is correct), but would
/// otherwise require use of `unsafe` to implement.
#[macro_export]
macro_rules! implement_into_ffi_by_delegation {
    ($SrcTy:ty, $MidTy:ty) => {
        unsafe impl $crate::IntoFfi for $SrcTy
        where
            $MidTy: From<$SrcTy> + $crate::IntoFfi,
        {
            // The <$MidTy as SomeTrait>::method is required even when it would
            // be ambiguous due to some obscure details of macro syntax.
            type Value = <$MidTy as $crate::IntoFfi>::Value;

            #[inline]
            fn ffi_default() -> Self::Value {
                <$MidTy as $crate::IntoFfi>::ffi_default()
            }

            #[inline]
            fn into_ffi_value(self) -> Self::Value {
                use $crate::IntoFfi;
                <$MidTy as From<$SrcTy>>::from(self).into_ffi_value()
            }
        }
    };
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
/// # use ffi_support::define_string_destructor;
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
/// # use ffi_support::define_box_destructor;
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

/// Define a (public) destructor for the ByteBuffer type.
///
/// ## Caveats
///
/// If you're using multiple separately compiled rust libraries in your application, it's critical
/// that you are careful to only ever free `ByteBuffer` instances allocated by a Rust library using
/// the same rust library. Passing them to a different Rust library's string destructor will cause
/// you to corrupt multiple heaps.
/// One common ByteBuffer destructor is defined per Rust library.
///
/// Also, to avoid name collisions, it is strongly recommended that you provide an name for this
/// function unique to your library. (This is true for all functions you expose).
///
/// ## Example
///
/// ```rust
/// # use ffi_support::define_bytebuffer_destructor;
/// define_bytebuffer_destructor!(mylib_destroy_bytebuffer);
/// ```
#[macro_export]
macro_rules! define_bytebuffer_destructor {
    ($destructor_name:ident) => {
        #[no_mangle]
        pub extern "C" fn $destructor_name(v: $crate::ByteBuffer) {
            v.destroy()
        }
    };
}

/// Define a (public) destructor for a type that lives inside a lazy_static
/// [`ConcurrentHandleMap`].
///
/// Note that this is actually totally safe, unlike the other
/// `define_blah_destructor` macros.
///
/// A critical difference, however, is that this dtor takes an `err` out
/// parameter to indicate failure. This difference is why the name is different
/// as well (deleter vs destructor).
///
/// ## Example
///
/// ```rust
/// # use lazy_static::lazy_static;
/// # use ffi_support::{ConcurrentHandleMap, define_handle_map_deleter};
/// struct Thing(Vec<i32>);
/// // Somewhere...
/// lazy_static! {
///     static ref THING_HANDLES: ConcurrentHandleMap<Thing> = ConcurrentHandleMap::new();
/// }
/// define_handle_map_deleter!(THING_HANDLES, mylib_destroy_thing);
/// ```
#[macro_export]
macro_rules! define_handle_map_deleter {
    ($HANDLE_MAP_NAME:ident, $destructor_name:ident) => {
        #[no_mangle]
        pub extern "C" fn $destructor_name(v: u64, err: &mut $crate::ExternError) {
            $crate::call_with_result(err, || {
                // Force type errors here.
                let map: &ConcurrentHandleMap<_> = &*$HANDLE_MAP_NAME;
                map.delete_u64(v)
            })
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
