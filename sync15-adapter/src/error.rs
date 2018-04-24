/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

error_chain! {
    foreign_links {
        Base64Decode(::base64::DecodeError);
        OpensslError(::openssl::error::ErrorStack);
        BadCleartextUtf8(::std::string::FromUtf8Error);
        JsonError(::serde_json::Error);
    }
    errors {
        BadKeyLength(which_key: &'static str, length: usize) {
            description("Incorrect key length")
            display("Incorrect key length for key {}: {}", which_key, length)
        }
        // Not including `expected` and `is`, since they don't seem useful and are inconvenient
        // to include. If we decide we want them it's not too bad to include.
        HmacMismatch {
            description("SHA256 HMAC Mismatch error")
            display("SHA256 HMAC Mismatch error")
        }

        // Used when a BSO should be decrypted but is encrypted, or vice versa.
        BsoWrongCryptState(is_decrypted: bool) {
            description("BSO in wrong encryption state for operation")
            display("Expected {} BSO, but got a(n) {} one",
                    if *is_decrypted { "encrypted" } else { "decrypted" },
                    if *is_decrypted { "decrypted" } else { "encrypted" })
        }
    }
}


