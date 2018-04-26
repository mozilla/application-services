/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub fn base16_encode(bytes: &[u8]) -> String {
    // This seems to be the fastest way of doing this without using a bunch of unsafe:
    // https://gist.github.com/thomcc/c4860d68cf31f9b0283c692f83a239f3
    static HEX_CHARS: &'static [u8] = b"0123456789abcdef";
    let mut result = vec![0u8; bytes.len() * 2];
    let mut index = 0;
    for &byte in bytes {
        result[index + 0] = HEX_CHARS[(byte >> 4) as usize];
        result[index + 1] = HEX_CHARS[(byte & 15) as usize];
        index += 2;
    }
    // We know statically that this unwrap is safe, since we can only write ascii
    String::from_utf8(result).unwrap()
}
