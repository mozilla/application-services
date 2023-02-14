/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[test]
fn tests() {
    let t = trybuild::TestCases::new();
    t.pass("parse.rs");
    t.compile_fail("returns_not_result.rs");
    t.compile_fail("returns_result_but_not_error.rs");
    t.compile_fail("returns_result_but_incorrect_error.rs");
    t.compile_fail("handle_error_on_non_functions.rs");
    t.compile_fail("macro_arguments.rs");
}
