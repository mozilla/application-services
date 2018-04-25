error_chain! {
  links {
    FxAClientError(::http_client::errors::Error, ::http_client::errors::ErrorKind);
  }
}
