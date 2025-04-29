pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Network error: {0}")]
    NetworkError(#[from] viaduct::Error),
    #[error("UTF-8 error: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),
    #[error("URL parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Status error: {0}")]
    StatusError(#[from] viaduct::UnexpectedStatus),
}
