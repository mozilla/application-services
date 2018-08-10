
extern crate fxa_client_ffi;
pub use fxa_client_ffi::*;

#[cfg(feature = "loginsapi")]
extern crate loginsapi_ffi;

#[cfg(feature = "loginsapi")]
pub use loginsapi_ffi::*;

/// A fake function that just reads the first byte of each of the functions
/// exposed by the FFI, so that the compiler (or linker, or whoever is
/// responsible for this) on android is forced to include them. AFAICT it's just
/// android that has this problem. It seems similar to rustlang issue 36342,
/// but maybe not the same? Dunno.
///
/// In either case, this works better than wrapping functions (despite being
/// basically the grosest thing ever), because
// 
/// - it doesn't require all the types in the crate in question be public (in
///   particular, fxa_client_ffi has some #[repr(C)] structs that are not
///   reexported).
/// - It also doesn't produce extra functions in the symbol table, nor does it require
///   updating when the arguments in question change.
///
/// It's certainly a lot less pretty though, and I'd *love* to find a way around this.
#[no_mangle]
pub unsafe extern "C" fn NEVER_CALL_THIS_workaround_rustlang_36342() {
    let mut functions = vec![
        &fxa_client_ffi::fxa_get_release_config as *const _ as *const u8,
        &fxa_client_ffi::fxa_get_custom_config as *const _ as *const u8,
        &fxa_client_ffi::fxa_new as *const _ as *const u8,
        &fxa_client_ffi::fxa_from_json as *const _ as *const u8,
        &fxa_client_ffi::fxa_to_json as *const _ as *const u8,
        &fxa_client_ffi::fxa_profile as *const _ as *const u8,
        &fxa_client_ffi::fxa_get_token_server_endpoint_url as *const _ as *const u8,
        &fxa_client_ffi::fxa_begin_oauth_flow as *const _ as *const u8,
        &fxa_client_ffi::fxa_complete_oauth_flow as *const _ as *const u8,
        &fxa_client_ffi::fxa_get_oauth_token as *const _ as *const u8,
        &fxa_client_ffi::fxa_str_free as *const _ as *const u8,
        &fxa_client_ffi::fxa_free as *const _ as *const u8,
        &fxa_client_ffi::fxa_config_free as *const _ as *const u8,
        &fxa_client_ffi::fxa_oauth_info_free as *const _ as *const u8,
        &fxa_client_ffi::fxa_profile_free as *const _ as *const u8,
        &fxa_client_ffi::fxa_sync_keys_free as *const _ as *const u8,
        &fxa_client_ffi::fxa_register_persist_callback as *const _ as *const u8,
        &fxa_client_ffi::fxa_unregister_persist_callback as *const _ as *const u8,
    ];
    // These are only present when fxa is built with the browserid feature.
    #[cfg(feature = "fxa_browserid")]
    {
        functions.extend(&[
            &fxa_client_ffi::fxa_from_credentials as *const _ as *const u8,
            &fxa_client_ffi::fxa_assertion_new as *const _ as *const u8,
            &fxa_client_ffi::fxa_get_sync_keys as *const _ as *const u8,
        ]);
    }
    #[cfg(feature = "loginsapi")]
    {
        functions.extend(&[
            &loginsapi_ffi::sync15_passwords_delete as *const _ as *const u8,
            &loginsapi_ffi::sync15_passwords_get_all as *const _ as *const u8,
            &loginsapi_ffi::sync15_passwords_get_by_id as *const _ as *const u8,
            &loginsapi_ffi::sync15_passwords_reset as *const _ as *const u8,
            &loginsapi_ffi::sync15_passwords_state_destroy as *const _ as *const u8,
            &loginsapi_ffi::sync15_passwords_state_new as *const _ as *const u8,
            &loginsapi_ffi::sync15_passwords_sync as *const _ as *const u8,
            &loginsapi_ffi::sync15_passwords_touch as *const _ as *const u8,
            &loginsapi_ffi::sync15_passwords_wipe as *const _ as *const u8,
            &loginsapi_ffi::wtf_destroy_c_char as *const _ as *const u8,
        ]);
    }
    for &i in functions.iter() {
        ::std::ptr::read_volatile::<u8>(i);
    }

    // The next block would also work, but seems like it's asking for trouble...

    // let mut p = functions.as_ptr();
    // let e = p.offset(functions.len() as isize);
    // while p != e {
    //     let f = ::std::ptr::read_volatile(p) as *const fn();
    //     p = p.offset(1);
    //     (*f)();
    // }
}
