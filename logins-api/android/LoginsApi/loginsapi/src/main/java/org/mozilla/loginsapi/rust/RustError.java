package org.mozilla.loginsapi.rust;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

import java.util.Arrays;
import java.util.List;

/**
 * This should be considered private, but it needs to be public for JNA.
 */
public class RustError extends Structure {
    public static class ByReference extends RustError implements Structure.ByReference {
    }

    public static class ByValue extends RustError implements Structure.ByValue {
    }
    // It's probably a mistake to touch this, but it needs to be public for JNA
    public Pointer message;

    /**
     * Does this represent success?
     */
    public boolean isSuccess() {
        return this.message == null;
    }

    /**
     * Does this represent failure?
     */
    public boolean isFailure() {
        return this.message != null;
    }

    /**
     * Get and consume the error message, or null if there is none.
     */
    public String consumeErrorMessage() {
        String result = this.getErrorMessage();
        if (this.message != null) {
            JNA.INSTANCE.destroy_c_char(this.message);
            this.message = null;
        }
        return result;
    }

    /**
     * Get the error message or null if there is none.
     */
    public String getErrorMessage() {
        return this.message == null ? null : this.message.getString(0, "utf8");
    }

    @Override
    protected List<String> getFieldOrder() {
        return Arrays.asList("message");
    }
}