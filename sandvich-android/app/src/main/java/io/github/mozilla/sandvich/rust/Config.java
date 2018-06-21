package io.github.mozilla.sandvich.rust;

import android.util.Log;

import com.sun.jna.Pointer;

public class Config extends RustObject {

    public Config(Pointer pointer) {
        this.rawPointer = pointer;
    }

    @Override
    public void close() {
        if (this.rawPointer != null) {
            JNA.INSTANCE.fxa_config_free(this.rawPointer);
        }
    }

    public static Config release() {
        RustResult result = JNA.INSTANCE.fxa_get_release_config();
        if (result.isSuccess()) {
            Pointer ptr = result.ok;
            result.ok = null;
            return new Config(ptr);
        } else {
            Log.e("Config.release", result.getError().message);
            return null;
        }
    }

    public static Config custom(String content_base) {
        RustResult result = JNA.INSTANCE.fxa_get_custom_config(content_base);
        if (result.isSuccess()) {
            Pointer ptr = result.ok;
            result.ok = null;
            return new Config(ptr);
        } else {
            Log.e("Config.custom", result.getError().message);
            return null;
        }
    }
}
