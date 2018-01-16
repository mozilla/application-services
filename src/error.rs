error_chain! {
  errors {
    LocalError(t: String) {
      description("FxA Local Error")
      display("Local Error Description: '{}'", t)
    }
    RemoteError(t: String) {
      description("FxA Remote Error")
      display("Remote Error Description: '{}'", t)
    }
  }
}
