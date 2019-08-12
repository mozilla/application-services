@file:Suppress("MaxLineLength")
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.logins.rust

import com.sun.jna.Library
import com.sun.jna.Pointer
import com.sun.jna.PointerType
import mozilla.appservices.support.native.loadIndirect
import org.mozilla.appservices.logins.BuildConfig

@Suppress("FunctionNaming", "FunctionParameterNaming", "LongParameterList", "TooGenericExceptionThrown")
internal interface PasswordSyncAdapter : Library {
    companion object {
        internal var INSTANCE: PasswordSyncAdapter =
            loadIndirect(componentName = "logins", componentVersion = BuildConfig.LIBRARY_VERSION)
    }

    fun sync15_passwords_state_new(
        mentat_db_path: String,
        encryption_key: String,
        error: RustError.ByReference
    ): LoginsDbHandle

    fun sync15_passwords_state_new_with_hex_key(
        db_path: String,
        encryption_key_bytes: ByteArray,
        encryption_key_len: Int,
        error: RustError.ByReference
    ): LoginsDbHandle

    fun sync15_passwords_state_destroy(handle: LoginsDbHandle, error: RustError.ByReference)

    // Important: strings returned from rust as *char must be Pointers on this end, returning a
    // String will work but either force us to leak them, or cause us to corrupt the heap (when we
    // free them).

    // Returns null if the id does not exist, otherwise json
    fun sync15_passwords_get_by_id(handle: LoginsDbHandle, id: String, error: RustError.ByReference): Pointer?

    // return json array
    fun sync15_passwords_get_all(handle: LoginsDbHandle, error: RustError.ByReference): Pointer?

    // Returns a JSON string containing a sync ping.
    fun sync15_passwords_sync(
        handle: LoginsDbHandle,
        key_id: String,
        access_token: String,
        sync_key: String,
        token_server_url: String,
        error: RustError.ByReference
    ): Pointer?

    fun sync15_passwords_wipe(handle: LoginsDbHandle, error: RustError.ByReference)
    fun sync15_passwords_wipe_local(handle: LoginsDbHandle, error: RustError.ByReference)
    fun sync15_passwords_reset(handle: LoginsDbHandle, error: RustError.ByReference)

    fun sync15_passwords_touch(handle: LoginsDbHandle, id: String, error: RustError.ByReference)
    // This is 1 for true and 0 for false, it would be a boolean but we need to return a value with
    // a known size.
    fun sync15_passwords_delete(handle: LoginsDbHandle, id: String, error: RustError.ByReference): Byte
    // Note: returns guid of new login entry (unless one was specifically requested)
    fun sync15_passwords_add(handle: LoginsDbHandle, new_login_json: String, error: RustError.ByReference): Pointer?
    fun sync15_passwords_update(handle: LoginsDbHandle, existing_login_json: String, error: RustError.ByReference)

    fun sync15_passwords_import_from_fennec(handle: LoginsDbHandle, db_path: String, error: RustError.ByReference)

    fun sync15_passwords_destroy_string(p: Pointer)

    fun sync15_passwords_new_interrupt_handle(handle: LoginsDbHandle, error: RustError.ByReference): RawLoginsInterruptHandle?
    fun sync15_passwords_interrupt(handle: RawLoginsInterruptHandle, error: RustError.ByReference)
    fun sync15_passwords_interrupt_handle_destroy(handle: RawLoginsInterruptHandle)
}

internal typealias LoginsDbHandle = Long

internal class RawLoginsInterruptHandle : PointerType()
