/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins.rust

import android.util.Log
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.PointerType
import java.lang.reflect.Proxy


@Suppress("FunctionNaming", "TooManyFunctions", "TooGenericExceptionThrown")
internal interface PasswordSyncAdapter : Library {
    companion object {
        private val JNA_LIBRARY_NAME = {
            val libname = System.getProperty("mozilla.appservices.logins_ffi_lib_name")
            if (libname != null) {
                Log.i("AppServices", "Using logins_ffi_lib_name: " + libname);
                libname
            } else {
                "logins_ffi"
            }
        }()

        internal var INSTANCE: PasswordSyncAdapter = try {
            Native.loadLibrary(JNA_LIBRARY_NAME, PasswordSyncAdapter::class.java) as PasswordSyncAdapter
        } catch (e: UnsatisfiedLinkError) {
            Proxy.newProxyInstance(
                    PasswordSyncAdapter::class.java.classLoader,
                    arrayOf(PasswordSyncAdapter::class.java))
            { _, _, _ ->
                throw RuntimeException("Logins storage functionality not available (no native library)", e)
            } as PasswordSyncAdapter
        }
    }

    fun sync15_passwords_state_new(
            mentat_db_path: String,
            encryption_key: String,
            error: RustError.ByReference
    ): RawLoginSyncState?

    fun sync15_passwords_state_destroy(p: RawLoginSyncState)

    // Important: strings returned from rust as *char must be Pointers on this end, returning a
    // String will work but either force us to leak them, or cause us to corrupt the heap (when we
    // free them).

    // Returns null if the id does not exist, otherwise json
    fun sync15_passwords_get_by_id(state: RawLoginSyncState, id: String, error: RustError.ByReference): Pointer?

    // return json array
    fun sync15_passwords_get_all(state: RawLoginSyncState, error: RustError.ByReference): Pointer?

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
    fun sync15_passwords_add(state: RawLoginSyncState, new_login_json: String, error: RustError.ByReference): Pointer?
    fun sync15_passwords_update(state: RawLoginSyncState, existing_login_json: String, error: RustError.ByReference)

    fun sync15_passwords_destroy_string(p: Pointer)
}

class RawLoginSyncState : PointerType()
