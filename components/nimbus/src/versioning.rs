/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! ## Nimbus SDK App Version Comparison
//! The Nimbus SDK supports comparing app versions that follow the Firefox versioning scheme.
//! This module was ported from the Firefox Desktop implementation. You can find the Desktop implementation
//! in [this C++ file](https://searchfox.org/mozilla-central/rev/468a65168dd0bc3c7d602211a566c16e66416cce/xpcom/base/nsVersionComparator.cpp)
//! There's also some more documentation in the [IDL](https://searchfox.org/mozilla-central/rev/468a65168dd0bc3c7d602211a566c16e66416cce/xpcom/base/nsIVersionComparator.idl#9-31)
//!
//! ## How versioning works
//! This module defines one main struct, the [`Version`] struct. A version is represented by a list of
//! dot separated **Version Parts**s. When comparing two versions, we compare each version part in order.
//! If one of the versions has a version part, but the other has run out (i.e we have reached the end of the list of version parts)
//! we compare the existing version part with the default version part, which is the `0`. For example,
//! `1.0` is equivalent to `1.0.0.0`.
//!
//! For information what version parts are composed of, and how they are compared, read the [next section](#the-version-part).
//!
//! ### Example Versions
//! The following are all valid versions:
//! - `1` (one version part, representing the `1`)
//! - `` (one version part, representing the empty string, which is equal to `0`)
//! - `12+` (one version part, representing `12+` which is equal to `13pre`)
//! - `98.1` (two version parts, one representing `98` and another `1`)
//! - `98.2pre1.0-beta` (three version parts, one for `98`, one for `2pre1` and one for `0-beta`)
//!
//!
//! ## The Version Part
//! A version part is made from 4 elements that directly follow each other:
//! - `num_a`: A 32-bit base-10 formatted number that is at the start of the part
//! - `str_b`: A non-numeric ascii-encoded string that starts after `num_a`
//! - `num_c`: Another 32-bit base-10 formatted number that follows `str_b`
//! - `extra_d`: The rest of the version part as an ascii-encoded string
//!
//! When two version parts are compared, each of `num_a`, `str_b`, `num_c` and `extra_d` are compared
//! in order. `num_a` and `num_c` are compared by normal integer comparison, `str_b` and `extra_b` are compared
//! by normal byte string comparison.
//!
//! ### Special values and cases
//! There two special characters that can be used in version parts:
//! 1. The `*`. This can be used to represent the whole version part. If used, it will set the `num_a` to be
//!     the maximum value possible ([`i32::MAX`]). This can only be used as the whole version part string. It will parsed
//!     normally as the `*` ascii character if it is preceded or followed by any other characters.
//! 1. The `+`. This can be used as the `str_b`. Whenever a `+` is used as a `str_b`, it increments the `num_a` by 1 and sets
//!     the `str_b` to be equal to `pre`. For example, `2+` is the same as `3pre`
//! 1. An empty `str_b` is always **greater** than a `str_b` with a value. For example, `93` > `93pre`
//!
//! ## Example version comparisons
//! The following comparisons are taken directly from [the brief documentation in Mozilla-Central](https://searchfox.org/mozilla-central/rev/468a65168dd0bc3c7d602211a566c16e66416cce/xpcom/base/nsIVersionComparator.idl#9-31)
//! ```
//! use nimbus::versioning::Version;
//! use std::convert::TryFrom;
//! let v1 = Version::try_from("1.0pre1").unwrap();
//! let v2 = Version::try_from("1.0pre2").unwrap();
//! let v3 = Version::try_from("1.0").unwrap();
//! let v4 = Version::try_from("1.0.0").unwrap();
//! let v5 = Version::try_from("1.0.0.0").unwrap();
//! let v6 = Version::try_from("1.1pre").unwrap();
//! let v7 = Version::try_from("1.1pre0").unwrap();
//! let v8 = Version::try_from("1.0+").unwrap();
//! let v9 = Version::try_from("1.1pre1a").unwrap();
//! let v10 = Version::try_from("1.1pre1").unwrap();
//! let v11 = Version::try_from("1.1pre10a").unwrap();
//! let v12 = Version::try_from("1.1pre10").unwrap();
//! assert!(v1 < v2);
//! assert!(v2 < v3);
//! assert!(v3 == v4);
//! assert!(v4 == v5);
//! assert!(v5 < v6);
//! assert!(v6 == v7);
//! assert!(v7 == v8);
//! assert!(v8 < v9);
//! assert!(v9 < v10);
//! assert!(v10 < v11);
//! assert!(v11 < v12);
//! ```
//! What the above is comparing is:
//! 1.0pre1
//! < 1.0pre2
//!   < 1.0 == 1.0.0 == 1.0.0.0
//!     < 1.1pre == 1.1pre0 == 1.0+
//!       < 1.1pre1a
//!         < 1.1pre1
//!           < 1.1pre10a
//!             < 1.1pre10

use std::{
    cmp::Ordering,
    convert::{TryFrom, TryInto},
};

use crate::NimbusError;

#[derive(Debug, Default, Clone, PartialEq)]
struct VersionPart {
    num_a: i32,
    str_b: String,
    num_c: i32,
    extra_d: String,
}

#[derive(Debug, Default, Clone)]
pub struct Version(Vec<VersionPart>);

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        let default_version_part: VersionPart = Default::default();
        let mut curr_idx = 0;
        while curr_idx < self.0.len() || curr_idx < other.0.len() {
            let version_part = self.0.get(curr_idx).unwrap_or(&default_version_part);
            let other_version_part = other.0.get(curr_idx).unwrap_or(&default_version_part);
            if !version_part.eq(other_version_part) {
                return false;
            }
            curr_idx += 1
        }
        true
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let mut idx = 0;
        let default_version: VersionPart = Default::default();
        while idx < self.0.len() || idx < other.0.len() {
            let version_part = self.0.get(idx).unwrap_or(&default_version);
            let other_version_part = other.0.get(idx).unwrap_or(&default_version);
            let ord = version_part.partial_cmp(other_version_part);
            match ord {
                Some(Ordering::Greater) | Some(Ordering::Less) => return ord,
                _ => (),
            }
            idx += 1;
        }
        Some(Ordering::Equal)
    }
}

impl PartialOrd for VersionPart {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let num_a_ord = self.num_a.partial_cmp(&other.num_a);
        match num_a_ord {
            Some(Ordering::Greater) | Some(Ordering::Less) => return num_a_ord,
            _ => (),
        };

        if self.str_b.is_empty() && !other.str_b.is_empty() {
            return Some(Ordering::Greater);
        } else if other.str_b.is_empty() && !self.str_b.is_empty() {
            return Some(Ordering::Less);
        }
        let str_b_ord = self.str_b.partial_cmp(&other.str_b);
        match str_b_ord {
            Some(Ordering::Greater) | Some(Ordering::Less) => return str_b_ord,
            _ => (),
        };

        let num_c_ord = self.num_c.partial_cmp(&other.num_c);
        match num_c_ord {
            Some(Ordering::Greater) | Some(Ordering::Less) => return num_c_ord,
            _ => (),
        };

        if self.extra_d.is_empty() && !other.extra_d.is_empty() {
            return Some(Ordering::Greater);
        } else if other.extra_d.is_empty() && !self.extra_d.is_empty() {
            return Some(Ordering::Less);
        }
        let extra_d_ord = self.extra_d.partial_cmp(&other.extra_d);
        match extra_d_ord {
            Some(Ordering::Greater) | Some(Ordering::Less) => return extra_d_ord,
            _ => (),
        };
        Some(Ordering::Equal)
    }
}

impl TryFrom<&'_ str> for Version {
    type Error = NimbusError;
    fn try_from(value: &'_ str) -> Result<Self, Self::Error> {
        let versions = value
            .split('.')
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Version(versions))
    }
}

impl TryFrom<String> for Version {
    type Error = NimbusError;
    fn try_from(curr_part: String) -> std::result::Result<Self, Self::Error> {
        curr_part.as_str().try_into()
    }
}

fn char_at(value: &str, idx: usize) -> Result<char, NimbusError> {
    value.chars().nth(idx).ok_or_else(|| {
        NimbusError::VersionParsingError(format!(
            "Tried to access character {} in string {}, but it has size {}",
            idx,
            value,
            value.len()
        ))
    })
}

fn is_num_c(c: char) -> bool {
    // TODO: why is the dash here?
    // this makes `1-beta` end up
    // having num_a = 1, str_b = "", num_c = 0 and extra_d = "-beta"
    // is that correct?
    // Taken from: https://searchfox.org/mozilla-central/rev/77efe87174ee82dad43da56d71a717139b9f19ee/xpcom/base/nsVersionComparator.cpp#107
    c.is_numeric() || c == '+' || c == '-'
}

fn parse_version_num(val: i32, res: &mut i32) -> Result<(), NimbusError> {
    if *res == 0 {
        *res = val;
    } else {
        let res_l = *res as i64;
        if (res_l * 10) + val as i64 > i32::MAX as i64 {
            return Err(NimbusError::VersionParsingError(
                "Number parsing overflows an i32".into(),
            ));
        }
        *res *= 10;
        *res += val;
    }
    Ok(())
}

impl TryFrom<&'_ str> for VersionPart {
    type Error = NimbusError;

    fn try_from(value: &'_ str) -> Result<Self, Self::Error> {
        if value.chars().any(|c| !c.is_ascii()) {
            return Err(NimbusError::VersionParsingError(format!(
                "version string {} contains non-ascii characters",
                value
            )));
        }
        if value.is_empty() {
            return Ok(Default::default());
        }

        let mut res: VersionPart = Default::default();
        // if the string value is the special "*",
        // then we set the num_a to be the highest possible value
        // handle that case before we start
        if value == "*" {
            res.num_a = i32::MAX;
            return Ok(res);
        }
        // Step 1: Parse the num_a, it's guaranteed to be
        // a base-10 number, if it exists
        let mut curr_idx = 0;
        while curr_idx < value.len() && char_at(value, curr_idx)?.is_numeric() {
            parse_version_num(
                char_at(value, curr_idx)?.to_digit(10).unwrap() as i32,
                &mut res.num_a,
            )?;
            curr_idx += 1;
        }
        if curr_idx >= value.len() {
            return Ok(res);
        }
        // Step 2: Parse the str_b. If str_b starts with a "+"
        // then we increment num_a, and set str_b to be "pre"
        let first_char = char_at(value, curr_idx)?;
        if first_char == '+' {
            res.num_a += 1;
            res.str_b = "pre".into();
            return Ok(res);
        }
        // otherwise, we parse until we either finish the string
        // or we find a numeric number, indicating the start of num_c
        while curr_idx < value.len() && !is_num_c(char_at(value, curr_idx)?) {
            res.str_b.push(char_at(value, curr_idx)?);
            curr_idx += 1;
        }

        if curr_idx >= value.len() {
            return Ok(res);
        }

        // Step 3: Parse the num_c, similar to how we parsed num_a
        while curr_idx < value.len() && char_at(value, curr_idx)?.is_numeric() {
            parse_version_num(
                char_at(value, curr_idx)?.to_digit(10).unwrap() as i32,
                &mut res.num_c,
            )?;
            curr_idx += 1;
        }
        if curr_idx >= value.len() {
            return Ok(res);
        }

        // Step 4: Assign all the remaining to extra_d
        res.extra_d = value[curr_idx..].into();
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;
    #[test]
    fn test_wild_card_to_version_part() -> Result<()> {
        let s = "*";
        let version_part = VersionPart::try_from(s)?;
        assert_eq!(version_part.num_a, i32::MAX);
        assert_eq!(version_part.str_b, "");
        assert_eq!(version_part.num_c, 0);
        assert_eq!(version_part.extra_d, "");
        Ok(())
    }

    #[test]
    fn test_empty_string_to_version_part() -> Result<()> {
        let s = "";
        let version_part = VersionPart::try_from(s)?;
        assert_eq!(version_part.num_a, 0);
        assert_eq!(version_part.str_b, "");
        assert_eq!(version_part.num_c, 0);
        assert_eq!(version_part.extra_d, "");
        Ok(())
    }

    #[test]
    fn test_only_num_a_to_version_part() -> Result<()> {
        let s = "98382";
        let version_part = VersionPart::try_from(s)?;
        assert_eq!(version_part.num_a, 98382);
        assert_eq!(version_part.str_b, "");
        assert_eq!(version_part.num_c, 0);
        assert_eq!(version_part.extra_d, "");
        Ok(())
    }

    #[test]
    fn test_num_a_and_plus_str_b() -> Result<()> {
        let s = "92+";
        let version_part = VersionPart::try_from(s)?;
        assert_eq!(version_part.num_a, 93);
        assert_eq!(version_part.str_b, "pre");
        assert_eq!(version_part.num_c, 0);
        assert_eq!(version_part.extra_d, "");
        Ok(())
    }

    #[test]
    fn test_num_a_and_str_b() -> Result<()> {
        let s = "92beta";
        let version_part = VersionPart::try_from(s)?;
        assert_eq!(version_part.num_a, 92);
        assert_eq!(version_part.str_b, "beta");
        assert_eq!(version_part.num_c, 0);
        assert_eq!(version_part.extra_d, "");
        Ok(())
    }

    #[test]
    fn test_num_a_str_b_and_num_c() -> Result<()> {
        let s = "92beta72";
        let version_part = VersionPart::try_from(s)?;
        assert_eq!(version_part.num_a, 92);
        assert_eq!(version_part.str_b, "beta");
        assert_eq!(version_part.num_c, 72);
        assert_eq!(version_part.extra_d, "");
        Ok(())
    }

    #[test]
    fn test_full_valid_string_to_version_part() -> Result<()> {
        let s = "1pre3extrabithere";
        let version_part = VersionPart::try_from(s)?;
        assert_eq!(version_part.num_a, 1);
        assert_eq!(version_part.str_b, "pre");
        assert_eq!(version_part.num_c, 3);
        assert_eq!(version_part.extra_d, "extrabithere");
        Ok(())
    }

    #[test]

    fn test_parse_full_version() -> Result<()> {
        let s = "92+.10.1.beta";
        let versions = Version::try_from(s.to_string())?;
        assert_eq!(
            vec![
                VersionPart {
                    num_a: 93,
                    str_b: "pre".into(),
                    ..Default::default()
                },
                VersionPart {
                    num_a: 10,
                    ..Default::default()
                },
                VersionPart {
                    num_a: 1,
                    ..Default::default()
                },
                VersionPart {
                    num_a: 0,
                    str_b: "beta".into(),
                    ..Default::default()
                }
            ],
            versions.0
        );
        Ok(())
    }

    #[test]
    fn test_compare_two_versions() -> Result<()> {
        let v1 = Version::try_from("92beta.1.2".to_string())?;
        let v2 = Version::try_from("92beta.1.2pre".to_string())?;
        assert!(v1 > v2);
        Ok(())
    }

    #[test]
    fn smoke_test_version_compare() -> Result<()> {
        // Test values from https://searchfox.org/mozilla-central/rev/5909d5b7f3e247dddff8229e9499db017eb438e2/xpcom/base/nsIVersionComparator.idl#24-31
        let v1 = Version::try_from("1.0pre1")?;
        let v2 = Version::try_from("1.0pre2")?;
        let v3 = Version::try_from("1.0")?;
        let v4 = Version::try_from("1.0.0")?;
        let v5 = Version::try_from("1.0.0.0")?;
        let v6 = Version::try_from("1.1pre")?;
        let v7 = Version::try_from("1.1pre0")?;
        let v8 = Version::try_from("1.0+")?;
        let v9 = Version::try_from("1.1pre1a")?;
        let v10 = Version::try_from("1.1pre1")?;
        let v11 = Version::try_from("1.1pre10a")?;
        let v12 = Version::try_from("1.1pre10")?;
        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 == v4);
        assert!(v4 == v5);
        assert!(v5 < v6);
        assert!(v6 == v7);
        assert!(v7 == v8);
        assert!(v8 < v9);
        assert!(v9 < v10);
        assert!(v10 < v11);
        assert!(v11 < v12);
        Ok(())
    }

    #[test]
    fn test_compare_wild_card() -> Result<()> {
        let v1 = Version::try_from("*")?;
        let v2 = Version::try_from("95.2pre")?;
        assert!(v1 > v2);
        Ok(())
    }

    #[test]
    fn test_non_ascii_throws_error() -> Result<()> {
        let err = Version::try_from("92🥲.1.2pre").expect_err("Should have thrown error");
        if let NimbusError::VersionParsingError(_) = err {
            // Good!
        } else {
            panic!("Expected VersionParsingError, got {:?}", err)
        }
        Ok(())
    }

    #[test]
    fn test_version_number_parsing_overflows() -> Result<()> {
        // This i32::MAX, should parse OK
        let v1 = VersionPart::try_from("2147483647")?;
        assert_eq!(v1.num_a, i32::MAX);
        // this is greater than i32::MAX, should return an error
        let err = VersionPart::try_from("2147483648")
            .expect_err("Should throw error, it overflows an i32");
        if let NimbusError::VersionParsingError(_) = err {
            // OK
        } else {
            panic!("Expected a VersionParsingError, got {:?}", err)
        }
        Ok(())
    }

    #[test]
    fn test_version_part_with_dashes() -> Result<()> {
        let v1 = VersionPart::try_from("0-beta")?;
        assert_eq!(
            VersionPart {
                num_a: 0,
                str_b: "".into(),
                num_c: 0,
                extra_d: "-beta".into(),
            },
            v1
        );
        Ok(())
    }

    #[test]
    fn test_exclamation_mark() -> Result<()> {
        let v1 = Version::try_from("93.!")?;
        let v2 = Version::try_from("93.1")?;
        let v3 = Version::try_from("93.0-beta")?;
        let v4 = Version::try_from("93.alpha")?;
        assert!(v1 < v2 && v1 < v3 && v1 < v4);
        Ok(())
    }
}
