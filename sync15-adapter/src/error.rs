/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_chain;

use base64;
use openssl;

error_chain! {
    foreign_links {
        Base64Decode(::base64::DecodeError);
        OpensslError(::openssl::error::ErrorStack);
        BadCleartextUtf8(::std::string::FromUtf8Error);
    }
    errors {
        BadKeyLength(which_key: &'static str, length: usize) {
            description("Incorrect key length")
            display("Incorrect key length for key {}: {}", which_key, length)
        }
    }
}


