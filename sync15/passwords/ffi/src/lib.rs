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
    with_translated_value_result,
    with_translated_void_result,
    with_translated_string_result,
    with_translated_opt_string_result,
};

use std::os::raw::{
    c_char,
};
use std::sync::{Once, ONCE_INIT};

use ffi_toolkit::string::{
    c_char_to_string,
};

pub use ffi_toolkit::memory::{
    destroy_c_char,
};

use sync::{
    Sync15StorageClient,
    Sync15StorageClientInit,
    GlobalState,
};
use sync15_passwords::{
    passwords,
    PasswordEngine,
    ServerPassword,
};

pub struct SyncInfo {
    state: GlobalState,
    client: Sync15StorageClient,
    // Used so that we know whether or not we need to re-initialize `client`
    last_client_init: Sync15StorageClientInit,
}

pub struct PasswordState {
    engine: PasswordEngine,
    sync: Option<SyncInfo>,
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

static INIT_LOGGER: Once = ONCE_INIT;
static DEV_LOGGER: &'static log::Log = &DevLogger;

fn init_logger() {
    log::set_logger(DEV_LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    std::env::set_var("RUST_BACKTRACE", "1");
    info!("Hooked up rust logger!");
}

define_destructor!(sync15_passwords_state_destroy, PasswordState);

// This is probably too many string arguments...
#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_state_new(
    mentat_db_path: *const c_char,
    encryption_key: *const c_char,
    error: *mut ExternError
) -> *mut PasswordState {
    INIT_LOGGER.call_once(init_logger);
    with_translated_result(error, || {

        let store = mentat::Store::open_with_key(c_char_to_string(mentat_db_path),
                                                 c_char_to_string(encryption_key))?;

        let engine = PasswordEngine::new(store)?;
        Ok(PasswordState {
            engine,
            sync: None,
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_sync(
    state: *mut PasswordState,
    key_id: *const c_char,
    access_token: *const c_char,
    sync_key: *const c_char,
    tokenserver_base_url: *const c_char,
    error: *mut ExternError
) {
    with_translated_void_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;

        let root_sync_key = sync::KeyBundle::from_ksync_base64(
            c_char_to_string(sync_key).into())?;

        let requested_init = Sync15StorageClientInit {
            key_id: c_char_to_string(key_id).into(),
            access_token: c_char_to_string(access_token).into(),
            tokenserver_base_url: c_char_to_string(tokenserver_base_url).into(),
        };

        // TODO: If `to_ready` (or anything else with a ?) fails below, this
        // `take()` means we end up with `state.sync.is_none()`, which means the
        // next sync will redownload meta/global, crypto/keys, etc. without
        // needing to. (AFAICT fixing this requires a change in sync15-adapter,
        // since to_ready takes GlobalState as a move, and it's not clear if
        // that change even is a good idea).
        let mut sync_info = state.sync.take().map(Ok)
                .unwrap_or_else(|| -> sync::Result<SyncInfo> {
            let state = GlobalState::default();
            let client = Sync15StorageClient::new(requested_init.clone())?;
            Ok(SyncInfo {
                state,
                client,
                last_client_init: requested_init.clone(),
            })
        })?;

        // If the options passed for initialization of the storage client aren't
        // the same as the ones we used last time, reinitialize it. (Note that
        // we could avoid the comparison in the case where we had `None` in
        // `state.sync` before, but this probably doesn't matter).
        if requested_init != sync_info.last_client_init {
            sync_info.client = Sync15StorageClient::new(requested_init.clone())?;
            sync_info.last_client_init = requested_init;
        }

        { // Scope borrow of `sync_info.client`
            let mut state_machine =
                sync::SetupStateMachine::for_readonly_sync(&sync_info.client, &root_sync_key);

            let next_sync_state = state_machine.to_ready(sync_info.state)?;
            sync_info.state = next_sync_state;
        }

        // We don't use a ? on the next line so that even if `state.engine.sync`
        // fails, we don't forget the sync_state.
        let result = state.engine.sync(&sync_info.client, &sync_info.state);
        state.sync = Some(sync_info);
        result
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_touch(state: *mut PasswordState, id: *const c_char, error: *mut ExternError) {
    with_translated_void_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        state.engine.touch_credential(c_char_to_string(id).into())?;
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_delete(state: *mut PasswordState, id: *const c_char, error: *mut ExternError) -> bool {
    with_translated_value_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        let deleted = state.engine.delete_credential(c_char_to_string(id).into())?;
        Ok(deleted)
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_wipe(state: *mut PasswordState, error: *mut ExternError) {
    with_translated_void_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        state.engine.wipe()?;
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_reset(state: *mut PasswordState, error: *mut ExternError) {
    with_translated_void_result(error, || {
        assert_pointer_not_null!(state);
        let state = &mut *state;
        state.engine.reset()?;
        // XXX We probably need to clear out some things from `state.service`!
        Ok(())
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_get_all(state: *mut PasswordState, error: *mut ExternError) -> *mut c_char {
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
pub unsafe extern "C" fn sync15_passwords_get_by_id(state: *mut PasswordState, id: *const c_char, error: *mut ExternError) -> *mut c_char {
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
