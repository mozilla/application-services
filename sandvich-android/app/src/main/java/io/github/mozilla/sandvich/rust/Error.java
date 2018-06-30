package io.github.mozilla.sandvich.rust;

import android.util.Log;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

import java.util.Arrays;
import java.util.List;

public class Error extends Structure {

    public static class ByReference extends Error implements Structure.ByReference {
    }

    public int code;
    // It's probably a mistake to touch this, but it needs to be public for JNA
    public Pointer message;


    /**
     * Does this represent success?
     */
    public boolean isSuccess() {
        return this.code == 0;
    }

    /**
     * Does this represent failure?
     */
    public boolean isFailure() {
        return this.code != 0;
    }

    /**
     * Get and consume the error message, or null if there is none.
     */
    public String consumeMessage() {
        String result = this.getMessage();
        if (this.message != null) {
            JNA.INSTANCE.fxa_str_free(this.message);
            this.message = null;
        }
        return result;
    }

    /**
     * Get the error message or null if there is none.
     */
    public String getMessage() {
        return this.message == null ? null : this.message.getString(0, "utf8");
    }

    @Override
    protected List<String> getFieldOrder() {
        return Arrays.asList("code", "message");
    }
}
