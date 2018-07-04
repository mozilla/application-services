/* -*- Mode: Java; c-basic-offset: 4; tab-width: 20; indent-tabs-mode: nil; -*-
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.loginsapi.rust;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.NativeLibrary;
import com.sun.jna.Pointer;
import com.sun.jna.PointerType;
import com.sun.jna.ptr.NativeLongByReference;

public interface JNA extends Library {
    String JNA_LIBRARY_NAME = "loginsapi_ffi";

    NativeLibrary JNA_NATIVE_LIB = NativeLibrary.getInstance(JNA_LIBRARY_NAME);

    JNA INSTANCE = (JNA) Native.loadLibrary(JNA_LIBRARY_NAME, JNA.class);
    // Important: If you're returning a string which later needs to be freed, do not return a
    // String! This will appear to work, but corrupt one or both heaps (rust doesn't use the native
    // heap by default). What's happening under the hood is JNA is copying the memory from the
    // pointer returned by your call into a Java String, and then promptly dropping that pointer on
    // the floor. This is a memory leak (but it gets worse). Then, when you destroy the String, it
    // does the same thing in reverse -- it allocates a temporary block of memory, copies the String
    // into it, and passes a pointer to that memory to native code. And then after the native code
    // returns, it frees said memory. The two problems with this are:
    //
    // 1. Rust uses a different heap than Android, so you're freeing a pointer on the wrong heap,
    //    which is extremely likely to corrupt both heaps.
    //
    // 2. Even if it weren't (you can configure Rust to use the System heap instead of using the
    //    Jemalloc heap), both you and the JNA code are freeing this memory. A double free! Fun.
    //
    // The way to avoid this is:
    //
    // ```java
    // // In JNA.java
    // Pointer thing_returning_str(/* args or whatever go here */);
    // void destroy_c_char(Pointer p);
    //
    // // In some utility code:
    // public static String getAndConsumeString(Pointer p) {
    //   Pointer p = JNA.INSTANCE.thing_returning_str(arg0, arg1, arg2);
    //   try {
    //     return p.readString(0, "utf8");
    //   } finally {
    //     JNI.INSTANCE.destroy_c_char(p);
    //   }
    // }
    //
    // // Usage code:
    // String myString = Util.getAndConsumeString(JNA.INSTANCE.thing_returning_str());
    // // ...
    // ```
    //
    // Note that this only applies to cases where you are later expected to free the pointer. If you
    // had a case where you don't need to free the pointer, it can safely return a String and
    // everything will "Just Work".
    //
    // That said, you will basically always be expected to free the pointer from Rust code. Rust's
    // strings are not null terminated, so we can just return a pointer to a string managed by rust,
    // we need to copy it into something else. We don't know when that copy will no longer be in use
    // by the caller, so it gets to be the caller's responsibility to free.

    class RawLoginSyncState extends PointerType {}

    RawLoginSyncState sync15_logins_state_new(
            String mentat_db_path,
            String metadata_path,
            String encryption_key,

            String key_id,
            String access_token,
            String sync_key,
            String token_server_base_url,

            RustError.ByReference error
    );

    void sync15_logins_state_destroy(RawLoginSyncState p);

    // Returns null if the id does not exist, otherwise json
    Pointer sync15_logins_get_by_id(RawLoginSyncState state, String id, RustError.ByReference error);

    // return json array
    Pointer sync15_logins_get_all(RawLoginSyncState state, RustError.ByReference error);

    void sync15_logins_sync(RawLoginSyncState state, RustError.ByReference error);

    void sync15_logins_wipe(RawLoginSyncState state, RustError.ByReference error);
    void sync15_logins_reset(RawLoginSyncState state, RustError.ByReference error);

    void sync15_logins_touch(RawLoginSyncState state, String id, RustError.ByReference error);
    void sync15_logins_delete(RawLoginSyncState state, String id, RustError.ByReference error);

    void destroy_c_char(Pointer p);

}
