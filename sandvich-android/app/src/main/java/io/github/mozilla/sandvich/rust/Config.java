package io.github.mozilla.sandvich.rust;

import android.util.Log;

import com.sun.jna.Pointer;

public class Config extends RustObject {

    public Config(Pointer pointer) {
        super(pointer);
    }

    @Override
    protected void destroyPointer(Pointer config) {
        JNA.INSTANCE.fxa_config_free(config);
    }

    public static Config release() {
        RustResult result = JNA.INSTANCE.fxa_get_release_config();
        result.logIfFailure("Config.release");
        if (result.isSuccess()) {
            return new Config(result.consumeSuccess());
        } else {
            return null;
        }
    }

    public static Config custom(String content_base) {
        RustResult result = JNA.INSTANCE.fxa_get_custom_config(content_base);
        result.logIfFailure("Config.custom");
        if (result.isSuccess()) {
            return new Config(result.consumeSuccess());
        } else {
            return null;
        }
    }
}
