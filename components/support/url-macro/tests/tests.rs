/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Note: the .stderr golden files in this directory are produced by
// trybuild and are sensitive to the rustc version. If they go stale
// after a toolchain bump, regenerate with `TRYBUILD=overwrite cargo
// test -p url-macro-tests` and review the diff before committing.

#[test]
fn tests() {
    let t = trybuild::TestCases::new();
    t.pass("pass.rs");
    t.compile_fail("empty_args.rs");
    t.compile_fail("non_string_literal.rs");
    t.compile_fail("invalid_url.rs");
}
