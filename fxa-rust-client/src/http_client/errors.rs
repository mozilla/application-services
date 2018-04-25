error_chain! {
  foreign_links {
    Base64Decode(::base64::DecodeError);
    OpensslError(::openssl::error::ErrorStack);
    BadCleartextUtf8(::std::string::FromUtf8Error);
    HexError(::hex::FromHexError);
    BadUrl(::reqwest::UrlError);
    RequestError(::reqwest::Error);
    HawkError(::hawk::Error);
  }
  errors {
    RemoteError(code: u64, errno: u64, error: String, message: String, info: String) {
      description("FxA Remote Error")
      display("Remote Error Description: '{}' '{}' '{}' '{}' '{}'", code, errno, error, message, info)
    }
    JsonError
  }
}
