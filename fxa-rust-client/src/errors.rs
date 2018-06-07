/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

error_chain! {
    foreign_links {
        BadCleartextUtf8(::std::string::FromUtf8Error);
        BadUrl(::reqwest::UrlError);
        Base64Decode(::base64::DecodeError);
        HawkError(::hawk::Error);
        HexError(::hex::FromHexError);
        JWTError(::jose::error::Error);
        JsonError(::serde_json::Error);
        OpensslError(::openssl::error::ErrorStack);
        RequestError(::reqwest::Error);
    }
    errors {
        RemoteError(code: u64, errno: u64, error: String, message: String, info: String) {
          description("FxA Remote Error")
          display("Remote Error Description: '{}' '{}' '{}' '{}' '{}'", code, errno, error, message, info)
        }
        NeededTokenNotFound {
            description("Required token needed for current operation not found.")
            display("Required token needed for current operation not found.")
        }
        UnknownOAuthState {
            description("Unknown OAuth state.")
            display("Unknown OAuth state.")
        }
        NotMarried {
            description("Not in a Married state.")
            display("Not in a Married state.")
        }
    }
}
