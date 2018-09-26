/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// TODO: share with logins_sql (this is taken from there).

use std::fmt;

#[derive(Debug, Clone)]
pub struct RepeatDisplay<'a, F> {
    count: usize,
    sep: &'a str,
    fmt_one: F
}

impl<'a, F> fmt::Display for RepeatDisplay<'a, F>
where F: Fn(usize, &mut fmt::Formatter) -> fmt::Result {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for i in 0..self.count {
            if i != 0 {
                f.write_str(self.sep)?;
            }
            (self.fmt_one)(i, f)?;
        }
        Ok(())
    }
}

pub fn repeat_display<'a, F>(count: usize, sep: &'a str, fmt_one: F) -> RepeatDisplay<'a, F>
where F: Fn(usize, &mut fmt::Formatter) -> fmt::Result {
    RepeatDisplay { count, sep, fmt_one }
}
