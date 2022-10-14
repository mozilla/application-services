/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[test]
fn tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/parse.rs");
    t.compile_fail("tests/return_not_result.rs");
    t.compile_fail("tests/err_not_internal.rs");
}
