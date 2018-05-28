package io.github.mozilla.sandvich.rust;

import com.sun.jna.Memory;
import com.sun.jna.Pointer;

import java.io.Closeable;
import java.nio.ByteBuffer;
import java.util.UUID;

/**
 * Base class that wraps an non-optional {@link Pointer} representing a pointer to a Rust object.
 * This class implements {@link Closeable} but does not provide an implementation, forcing all
 * subclasses to implement it. This ensures that all classes that inherit from RustObject
 * will have their {@link Pointer} destroyed when the Java wrapper is destroyed.
 */
abstract class RustObject implements Closeable {
    Pointer rawPointer;

    /**
     * Throws a {@link NullPointerException} if the underlying {@link Pointer} is null.
     */
    void validate() {
        if (this.rawPointer == null) {
            throw new NullPointerException(this.getClass() + " consumed");
        }
    }

}
