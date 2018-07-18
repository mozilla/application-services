// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

// We take the "low road" here when returning the structs - we expose the
// items (and arrays of items) as strings, which are JSON. The rust side of
// the world gets serialization and deserialization for free and it makes
// memory management that little bit simpler.

extern crate failure;
extern crate serde_json;

#[macro_use] extern crate ffi_toolkit;
extern crate mentat;
extern crate sync15_passwords;
extern crate sync15_adapter as sync;
#[macro_use] extern crate log;

mod error;

use error::{
    ExternError,
    with_translated_result,
    with_translated_void_result,
    with_translated_string_result,
    with_translated_opt_string_result,
};

use std::os::raw::{
    c_char,
};

use ffi_toolkit::string::{
    c_char_to_string,
};

pub use ffi_toolkit::memory::{
    destroy_c_char,
};

use sync::{
    Sync15Service,
    Sync15ServiceInit,
};
use sync15_passwords::{
    passwords,
    PasswordEngine,
    ServerPassword,
};

pub struct PasswordSyncState {
    engine: PasswordEngine,
    service: sync::Sync15Service,
}

#[cfg(target_os = "android")]
extern { pub fn __android_log_write(level: ::std::os::raw::c_int, tag: *const c_char, text: *const c_char) -> ::std::os::raw::c_int; }

struct DevLogger;
impl log::Log for DevLogger {
    fn enabled(&self, _: &log::Metadata) -> bool { true }
    fn log(&self, record: &log::Record) {
        let message = format!("{}:{} -- {}", record.level(), record.target(), record.args());
        println!("{}", message);
        #[cfg(target_os = "android")]
        {
            unsafe {
                let message = ::std::ffi::CString::new(message).unwrap();
                let level_int = match record.level() {
                    log::Level::Trace => 2,
                    log::Level::Debug => 3,
                    log::Level::Info => 4,
                    log::Level::Warn => 5,
                    log::Level::Error => 6,
                };
                let message = message.as_ptr();
                let tag = b"RustInternal\0";
                __android_log_write(level_int, tag.as_ptr() as *const c_char, message);
            }
        }
        // TODO ios (use NSLog(__CFStringMakeConstantString(b"%s\0"), ...), maybe windows? (OutputDebugStringA)
    }
    fn flush(&self) {}
}
static DEV_LOGGER: &'static log::Log = &DevLogger;
fn init_logger() {
    log::set_logger(DEV_LOGGER).unwrap();
    info!("Hooked up rust logger!");
}

define_destructor!(sync15_passwords_state_destroy, PasswordSyncState);

// This is probably too many string arguments...
#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_state_new(
    mentat_db_path: *const c_char,

    encryption_key: *const c_char,

    key_id: *const c_char,
    access_token: *const c_char,
    sync_key: *const c_char,
    tokenserver_base_url: *const c_char,

    error: *mut ExternError
) -> *mut PasswordSyncState {
    init_logger();
    with_translated_result(error, || {
        let params = Sync15ServiceInit {
            key_id: c_char_to_string(key_id).into(),
            access_token: c_char_to_string(access_token).into(),
            sync_key: c_char_to_string(sync_key).into(),
            tokenserver_base_url: c_char_to_string(tokenserver_base_url).into(),
        };

        let mut service = Sync15Service::new(params)?;
        service.remote_setup()?;

        let store = mentat::Store::open_with_key(c_char_to_string(mentat_db_path),
                                                 c_char_to_string(encryption_key))?;

        let engine = PasswordEngine::new(store)?;
        Ok(PasswordSyncState { service, engine })
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_sync(state: *mut PasswordSyncState, error: *mut ExternError) {
    with_translated_void_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        state.engine.sync(&state.service)?;
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_touch(state: *mut PasswordSyncState, id: *const c_char, error: *mut ExternError) {
    with_translated_void_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        state.engine.touch_credential(c_char_to_string(id).into())?;
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_delete(state: *mut PasswordSyncState, id: *const c_char, error: *mut ExternError) {
    with_translated_void_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        {
            let mut in_progress = state.engine.store.begin_transaction()?;
            passwords::delete_by_sync_uuid(&mut in_progress, c_char_to_string(id).into())?;
            in_progress.commit()?;
        }
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_wipe(state: *mut PasswordSyncState, error: *mut ExternError) {
    with_translated_void_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        state.engine.wipe()?;
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_reset(state: *mut PasswordSyncState, error: *mut ExternError) {
    with_translated_void_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        state.engine.reset()?;
        // XXX We probably need to clear out some things from `state.service`!
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_get_all(state: *mut PasswordSyncState, error: *mut ExternError) -> *mut c_char {
    with_translated_string_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        // Type declaration is just to make sure we have the right type (and for documentation)
        let passwords: Vec<ServerPassword> = {
            let mut in_progress_read = state.engine.store.begin_read()?;
            passwords::get_all_sync_passwords(&mut in_progress_read)?
        };
        let result = serde_json::to_string(&passwords)?;
        Ok(result)
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_get_by_id(state: *mut PasswordSyncState, id: *const c_char, error: *mut ExternError) -> *mut c_char {
    with_translated_opt_string_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        // Type declaration is just to make sure we have the right type (and for documentation)
        let maybe_pass: Option<ServerPassword> = {
            let mut in_progress_read = state.engine.store.begin_read()?;
            passwords::get_sync_password(&mut in_progress_read, c_char_to_string(id).into())?
        };
        let pass = if let Some(p) = maybe_pass { p } else {
            return Ok(None)
        };
        Ok(Some(serde_json::to_string(&pass)?))
    })
}

#[no_mangle]
pub extern "C" fn wtf_destroy_c_char(s: *mut c_char) {
    // the "pub use" above should should be enough to expose this?
    // It appears that is enough to expose it in a windows DLL, but for
    // some reason it's not expored for Android.
    // *sob* - and now that I've defined this, suddenly this *and*
    // destroy_c_char are exposed (and removing this again removes the
    // destroy_c_char)
    // Oh well, a yak for another day.
    destroy_c_char(s);
}
