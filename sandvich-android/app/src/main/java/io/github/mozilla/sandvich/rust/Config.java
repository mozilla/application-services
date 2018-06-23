package io.github.mozilla.sandvich.rust;

import android.util.Log;

import com.sun.jna.Pointer;

public class Config extends RustObject<JNA.RawConfig> {
    public Config(JNA.RawConfig pointer) {
        this.rawPointer = pointer;
    }

    @Override
    protected void destroyPointer(JNA.RawConfig cfg) {
        JNA.INSTANCE.fxa_config_free(cfg);
    }

    public static Config release() {
        Error.ByReference e = new Error.ByReference();
        JNA.RawConfig cfg = JNA.INSTANCE.fxa_get_release_config(e);
        if (e.isSuccess()) {
            return new Config(cfg);
        } else {
            Log.e("Config.release", e.consumeMessage());
            return null;
        }
    }

    public static Config custom(String content_base) {
        Error.ByReference e = new Error.ByReference();
        JNA.RawConfig cfg = JNA.INSTANCE.fxa_get_custom_config(content_base, e);
        if (e.isSuccess()) {
            return new Config(cfg);
        } else {
            Log.e("Config.custom", e.consumeMessage());
            return null;
        }
    }
}
