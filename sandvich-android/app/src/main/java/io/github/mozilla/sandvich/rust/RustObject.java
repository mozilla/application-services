package io.github.mozilla.sandvich.rust;

import android.util.Log;

import com.sun.jna.Pointer;

import java.io.Closeable;

/**
 * Base class that wraps an non-optional {@link Pointer} representing a pointer to a Rust object.
 * This class implements {@link Closeable} but does not provide an implementation, forcing all
 * subclasses to implement it. This ensures that all classes that inherit from RustObject
 * will have their {@link Pointer} destroyed when the Java wrapper is destroyed.
 */
abstract class RustObject<T> implements Closeable {
    public T rawPointer;

    /**
     * Throws a {@link NullPointerException} if the underlying {@link Pointer} is null.
     */
    void validate() {
        if (this.rawPointer == null) {
            throw new NullPointerException(this.getClass() + " consumed");
        }
    }

    T validPointer() {
        this.validate();
        return this.rawPointer;
    }

    boolean isConsumed() {
        return this.rawPointer == null;
    }

    T consumePointer() {
        this.validate();
        T p = this.rawPointer;
        this.rawPointer = null;
        return p;
    }

    /* package-local */
    static String getAndConsumeString(Pointer stringPtr) {
        if (stringPtr == null) {
            return null;
        }
        try {
            String str = stringPtr.getString(0, "utf8");
            Log.e("getcons",str);
            return str;
        } finally {
            JNA.INSTANCE.fxa_str_free(stringPtr);
        }
    }

    abstract protected void destroyPointer(T p);

    @Override
    public void close() {
        if (this.rawPointer != null) {
            this.destroyPointer(this.consumePointer());
        }
    }

    @Override
    protected void finalize() {
        try {
            this.close();
        } catch (Exception e) {
            Log.e("RustObject", e.toString());
        }
    }

}
