use std::fmt;

#[derive(Debug, Clone)]
pub struct ErrorStack {}

impl std::error::Error for ErrorStack {
    fn description(&self) -> &str {
        fatal()
    }
}

impl fmt::Display for ErrorStack {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        fatal()
    }
}

pub fn fatal() -> ! {
    panic!("Attempted to use stubbed-out OpenSSL");
}
