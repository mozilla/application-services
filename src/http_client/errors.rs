error_chain! {
  errors {
    RemoteError(code: u64, errno: u64, error: String, message: String, info: String) {
      description("FxA Remote Error")
      display("Remote Error Description: '{}' '{}' '{}' '{}' '{}'", code, errno, error, message, info)
    }
  }
}
