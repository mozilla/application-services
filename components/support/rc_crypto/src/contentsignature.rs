/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use hex;

fn decode_root_hash(input: &str) -> Result<Vec<u8>> {
    let bytes_hex = input.split(':');

    let mut result: Vec<u8> = vec![];
    for byte_hex in bytes_hex {
        let byte = match hex::decode(&byte_hex) {
            Ok(v) => v,
            Err(_) => return Err(ErrorKind::RootHashFormatError(input.to_string()).into()),
        };
        result.extend(byte);
    }

    Ok(result)
}

#[cfg(test)]
mod test {
    use super::*;

    const ROOT_HASH: &str = "3C:01:44:6A:BE:90:36:CE:A9:A0:9A:CA:A3:A5:20:AC:62:8F:20:A7:AE:32:CE:86:1C:B2:EF:B7:0F:A0:C7:45";

    #[test]
    fn test_decode_root_hash() {
        assert!(decode_root_hash("meh!").is_err());
        assert!(decode_root_hash("3C:rr:44").is_err());

        let result = decode_root_hash(ROOT_HASH).unwrap();
        assert_eq!(
            result,
            vec![
                60, 1, 68, 106, 190, 144, 54, 206, 169, 160, 154, 202, 163, 165, 32, 172, 98, 143,
                32, 167, 174, 50, 206, 134, 28, 178, 239, 183, 15, 160, 199, 69
            ]
        );
    }
}
