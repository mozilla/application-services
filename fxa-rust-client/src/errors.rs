error_chain! {
    foreign_links {
        Base64Decode(::base64::DecodeError);
        OpensslError(::openssl::error::ErrorStack);
        BadUrl(::reqwest::UrlError);
        BadCleartextUtf8(::std::string::FromUtf8Error);
        HexError(::hex::FromHexError);
        JsonError(::serde_json::Error);
        RequestError(::reqwest::Error);
        HawkError(::hawk::Error);
        JWTError(::jose::error::Error);
    }
    errors {
        RemoteError(code: u64, errno: u64, error: String, message: String, info: String) {
          description("FxA Remote Error")
          display("Remote Error Description: '{}' '{}' '{}' '{}' '{}'", code, errno, error, message, info)
        }
        NeededTokenNotFound {
            description("Needed token not found")
            display("Needed token not found.")
        }
        UnknownOAuthState {
            description("Unknown OAuth state")
            display("Unknown OAuth state.")
        }
        NotMarried {
            description("Not in a Married state.")
            display("Not in a Married state.")
        }
        NoSessionToken {
            description("Not in a session token state.")
            display("Not in a session token state.")
        }
    }
}
