/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.rustlog;

import android.util.Log;

import com.sun.jna.Callback;
import com.sun.jna.Native;
import com.sun.jna.Pointer;
import com.sun.jna.PointerType;

@SuppressWarnings("JniMissingFunction")
class LibRustLogAdapter {

    static {
        Native.register(jnaLibraryName());
    }

    private static String jnaLibraryName() {
        String libname = System.getProperty("mozilla.appservices.ac_rust_log_lib_name");
        if (libname != null) {
            Log.i("AppServices", "Using ac_rust_log_lib_name: " + libname);
            return libname;
        } else {
            return "ac_rust_log";
        }
    }

    static native RawLogAdapter ac_log_adapter_create(
            RawLogCallback callback,
            RustError.ByReference out_err
    );

    static native RawLogAdapter ac_log_adapter_set_max_level(
            RawLogAdapter adapter,
            int level,
            RustError.ByReference out_err
    );

    static native RawLogAdapter ac_log_adapter_destroy(
            RawLogAdapter adapter
    );

    static native RawLogAdapter ac_log_adapter_destroy_string(
            Pointer stringPtr
    );

    static native RawLogAdapter ac_log_adapter_test__log_msg(
            String string
    );

    public interface RawLogCallback extends Callback {
        byte invoke(int level, Pointer tag, Pointer message);
    }

    public static class RawLogAdapter extends PointerType {
        public RawLogAdapter() {}
    }
}
