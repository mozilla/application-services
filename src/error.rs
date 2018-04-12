error_chain! {
  errors {
    LocalError(t: String) {
      description("FxA Local Error")
      display("Local Error Description: '{}'", t)
    }
    RemoteError(code: u64, errno: u64, error: String, message: String, info: String) {
      description("FxA Remote Error")
      display("Remote Error Description: '{}' '{}' '{}' '{}' '{}'", code, errno, error, message, info)
    }
  }
}
