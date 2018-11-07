/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package mozilla.appservices.logins.rust
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.PointerType


@Suppress("FunctionNaming", "TooManyFunctions", "TooGenericExceptionThrown")
internal interface PasswordSyncAdapter : Library {
    companion object {
        private const val JNA_LIBRARY_NAME = "loginsapi_ffi"
        internal var INSTANCE: PasswordSyncAdapter

        init {
            INSTANCE = Native.loadLibrary(JNA_LIBRARY_NAME, PasswordSyncAdapter::class.java) as PasswordSyncAdapter
        }
    }

    fun sync15_passwords_state_new(
            mentat_db_path: String,
            encryption_key: String,
            error: RustError.ByReference
    ): RawLoginSyncState

    fun sync15_passwords_state_destroy(p: RawLoginSyncState)

    // Important: strings returned from rust as *char must be Pointers on this end, returning a
    // String will work but either force us to leak them, or cause us to corrupt the heap (when we
    // free them).

    // Returns null if the id does not exist, otherwise json
    fun sync15_passwords_get_by_id(state: RawLoginSyncState, id: String, error: RustError.ByReference): Pointer

    // return json array
    fun sync15_passwords_get_all(state: RawLoginSyncState, error: RustError.ByReference): Pointer

    fun sync15_passwords_sync(state: RawLoginSyncState,
                              key_id: String,
                              access_token: String,
                              sync_key: String,
                              token_server_url: String,
                              error: RustError.ByReference)

    fun sync15_passwords_wipe(state: RawLoginSyncState, error: RustError.ByReference)
    fun sync15_passwords_reset(state: RawLoginSyncState, error: RustError.ByReference)

    fun sync15_passwords_touch(state: RawLoginSyncState, id: String, error: RustError.ByReference)
    // This is 1 for true and 0 for false, it would be a boolean but we need to return a value with
    // a known size.
    fun sync15_passwords_delete(state: RawLoginSyncState, id: String, error: RustError.ByReference): Byte
    // Note: returns guid of new login entry (unless one was specifically requested)
    fun sync15_passwords_add(state: RawLoginSyncState, new_login_json: String, error: RustError.ByReference): Pointer
    fun sync15_passwords_update(state: RawLoginSyncState, existing_login_json: String, error: RustError.ByReference)

    fun sync15_passwords_destroy_string(p: Pointer)
}

class RawLoginSyncState : PointerType()
