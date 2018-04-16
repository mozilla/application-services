error_chain! {
  links {
    FxAClientError(::fxa_client::errors::Error, ::fxa_client::errors::ErrorKind);
  }
}
