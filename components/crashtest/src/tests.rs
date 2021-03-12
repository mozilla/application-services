/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// The tests are in a separate file just to ensure that the
// main `lib.rs` contains nothing but the public interface.

#[test]
#[should_panic]
fn test_trigger_panic() {
    crate::trigger_rust_panic();
}

#[test]
fn test_trigger_error() {
    assert!(matches!(
        crate::trigger_rust_error(),
        Err(crate::CrashTestError::ErrorFromTheRustCode)
    ));
}

// We can't test `trigger_rust_abort()` here because it's a hard error.
