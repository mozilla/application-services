/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

protocol Destroyable {
    associatedtype T
    func cleanup(pointer: T)
}

public typealias RustStructPointer<T> = RustPointer<UnsafeMutablePointer<T>>
public typealias RustOpaquePointer = RustPointer<OpaquePointer>
public typealias MovableRustStructPointer<T> = MovableRustPointer<UnsafeMutablePointer<T>>
public typealias MovableRustOpaquePointer = MovableRustPointer<OpaquePointer>

/**
 Base class that wraps an optional pointer to a Rust object.
 This class provides cleanup functions on deinit, ensuring that all classes
 that inherit from it will have their pointer destroyed when the Swift wrapper is destroyed.
 If a class does not override `cleanup` then a `fatalError` is thrown.
 */
public class RustPointer<T>: Destroyable {
    var raw: T

    init(raw: T) {
        self.raw = raw
    }

    deinit {
        self.cleanup(pointer: self.raw)
    }

    func cleanup(pointer: T) {
        fatalError("\(cleanup) is not implemented.")
    }
}

/**
 Base class that wraps an optional pointer to a Rust object.
 This class should be used to wrap Rust pointer that point to consuming structs, that is, calling a function
 for that Rust pointer, will cause Rust to destroy the pointer, leaving the Swift pointer dangling.
 These classes are responsible for ensuring that their raw pointer are `nil`led after calling a consuming
 FFI function.
 This class provides cleanup functions on deinit, ensuring that all classes
 that inherit from it will have their pointer destroyed when the Swift wrapper is destroyed.
 If a class does not override `cleanup` then a `fatalError` is thrown.
 The optional pointer is managed here such that is the pointer is nil, then the cleanup function is not called
 ensuring that we do not double free the pointer on exit.
 */
public class MovableRustPointer<T>: Destroyable {
    var raw: T?

    init(raw: T) {
        self.raw = raw
    }

    deinit {
        guard let raw = self.raw else { return }
        self.cleanup(pointer: raw)
    }

    /**
     Provides a non-optional `OpaquePointer` if one exists for this class.
     - Throws: `Pointer.pointerConsumed` if the raw pointer wrapped by this class is nil
     - Returns: the raw `OpaquePointer` wrapped by this class.
     */
    func validPointer() throws -> T {
        guard let r = self.raw else {
            throw PointerError.pointerConsumed
        }
        return r
    }

    /** Equivalent to validPointer, but clears `self.raw` after use */
    func movePointer() throws -> T {
        guard let r = self.raw else {
            throw PointerError.pointerConsumed
        }
        self.raw = nil
        return r
    }

    func cleanup(pointer: T) {
        fatalError("\(cleanup) is not implemented.")
    }
}
