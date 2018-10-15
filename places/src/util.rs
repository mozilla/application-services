/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */


use unicode_segmentation::UnicodeSegmentation;
use unicode_normalization::UnicodeNormalization;

use caseless::Caseless;


/// Performs case folding and NFKD normalization on `s`.
pub fn unicode_normalize(s: &str) -> String {
    s.chars().default_case_fold().nfkd().collect()
}

/// Equivalent to `&s[..max_len.min(s.len())]`, but handles the case where
/// `s.is_char_boundary(max_len)` is false (which would otherwise panic).
pub fn slice_up_to(s: &str, max_len: usize) -> &str {
    if max_len >= s.len() {
        return s;
    }
    let mut idx = max_len;
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

/// Performs (an operation equivalent to) [`unicode_normalize`] on `text`, and replaces
/// ensures that the true `unicode_words()` value can be recovered by splitting on `' '`.
pub fn to_normalized_words(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for (i, word) in text.unicode_words().enumerate() {
        if i > 0 {
            result.push(' ');
        }
        result.extend(word.chars().default_case_fold().nfkd());
    }
    result
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_slice_up_to() {
        assert_eq!(slice_up_to("abcde", 4), "abcd");
        assert_eq!(slice_up_to("abcde", 5), "abcde");
        assert_eq!(slice_up_to("abcde", 6), "abcde");
        let s = "abcd😀";
        assert_eq!(s.len(), 8);
        assert_eq!(slice_up_to(s, 4), "abcd");
        assert_eq!(slice_up_to(s, 5), "abcd");
        assert_eq!(slice_up_to(s, 6), "abcd");
        assert_eq!(slice_up_to(s, 7), "abcd");
        assert_eq!(slice_up_to(s, 8), s);
    }
}



