/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

error_chain! {
    foreign_links {
        Base64Decode(::base64::DecodeError);
        OpensslError(::openssl::error::ErrorStack);
        BadCleartextUtf8(::std::string::FromUtf8Error);
        JsonError(::serde_json::Error);
        BadUrl(::reqwest::UrlError);
        RequestError(::reqwest::Error);
        HawkError(::hawk::Error);
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

        // Error from tokenserver. Ideally we should probably do a better job here...
        TokenserverHttpError(code: ::reqwest::StatusCode) {
            description("HTTP status when requesting a token from the tokenserver")
            display("HTTP status {} when requesting a token from the tokenserver", code)
        }

        // As above, but for storage requests
        StorageHttpError(code: ::reqwest::StatusCode, route: String) {
            description("HTTP error status when making a request to storage server")
            display("HTTP status {} during a storage request to \"{}\"", code, route)
        }

        BackoffError(retry_after_secs: f64) {
            description("Server requested backoff")
            display("Server requested backoff. Retry after {} seconds.", retry_after_secs)
        }

        // This might just be a NYI, since IDK if we want to upload this.
        NoMetaGlobal {
            description("No meta global on server for user")
            display("No meta global on server for user")
        }

        // We should probably get rid of the ones of these that are actually possible,
        // but I'd like to get this done rather than spend tons of time worrying about
        // the right error types for now (but at the same time, I'd rather not unwrap)
        UnexpectedError(message: String) {
            description("Unexpected error")
            display("Unexpected error: {}", message)
        }

        RecordTooLargeError {
            description("Record is larger than the maximum size allowed by the server")
            display("Record is larger than the maximum size allowed by the server")
        }
    }
}

// Boilerplate helper...
pub fn unexpected<S>(s: S) -> Error where S: Into<String> {
    ErrorKind::UnexpectedError(s.into()).into()
}


