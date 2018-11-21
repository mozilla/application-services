/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Returns true for Guids that are valid places guids, and false for all others.
pub fn is_valid_places_guid(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(is_valid_places_byte)
}

/// Returns true if the byte `b` is a valid base64url byte.
#[inline]
pub fn is_valid_places_byte(b: u8) -> bool {
    BASE64URL_BYTES[b as usize] == 1
}

// This is used to implement the places tests.
const BASE64URL_BYTES: [u8; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1,
    0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_base64url_bytes() {
        let mut expect = [0u8; 256];
        for b in b'0'..=b'9' {
            expect[b as usize] = 1;
        }
        for b in b'a'..=b'z' {
            expect[b as usize] = 1;
        }
        for b in b'A'..=b'Z' {
            expect[b as usize] = 1;
        }
        expect[b'_' as usize] = 1;
        expect[b'-' as usize] = 1;
        assert_eq!(&BASE64URL_BYTES[..], &expect[..]);
    }

    #[test]
    fn test_valid_for_places() {
        assert!(is_valid_places_guid("aaaabbbbcccc"));
        assert!(is_valid_places_guid("09_az-AZ_09-"));
        assert!(!is_valid_places_guid("aaaabbbbccccd")); // too long
        assert!(!is_valid_places_guid("aaaabbbbccc")); // too short
        assert!(!is_valid_places_guid("aaaabbbbccc=")); // right length, bad character
    }
}