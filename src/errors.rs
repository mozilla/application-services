error_chain! {
  foreign_links {
    HexError(::hex::FromHexError);
    JsonError(::serde_json::Error);
  }
  links {
    HTTPClientError(::http_client::errors::Error, ::http_client::errors::ErrorKind);
  }
  errors {
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
