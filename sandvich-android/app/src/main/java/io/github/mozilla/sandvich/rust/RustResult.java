package io.github.mozilla.sandvich.rust;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

import java.io.Closeable;
import java.io.IOException;
import java.util.Arrays;
import java.util.List;

/**
 * Represents a C struct containing a {@link Pointer}s and String that map to a Rust Result.
 * A RustResult will contain either an ok value, OR an err value, or neither - never both.
 */
public class RustResult extends Structure implements Closeable {
    public static class ByReference extends RustResult implements Structure.ByReference {
    }

    public static class ByValue extends RustResult implements Structure.ByValue {
    }

    public Pointer ok;
    public Pointer err;

    public Error getError () {
        return new Error(this.err);
    }

    /**
     * Is there an value attached to this result
     * @return  true if a value is present, false otherwise
     */
    public boolean isSuccess() {
        return this.ok != null;
    }

    /**
     * Is there an error attached to this result?
     * @return  true is an error is present, false otherwise
     */
    public boolean isFailure() {
        return this.err != null;
    }

    @Override
    protected List<String> getFieldOrder() {
        return Arrays.asList("ok", "err");
    }

    @Override
    public void close() throws IOException {
        if (this.getPointer() != null) {
            // TODO:
            // JNA.INSTANCE.destroy(this.getPointer());
        }
    }
}