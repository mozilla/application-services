#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("Invalid login: {0}")]
    InvalidLogin(InvalidLogin),

    #[error("The `sync_status` column in DB has an illegal value: {0}")]
    BadSyncStatus(u8),

    #[error("A duplicate GUID is present: {0:?}")]
    DuplicateGuid(String),

    #[error("No record with guid exists (when one was required): {0:?}")]
    NoSuchRecord(String),

    // Fennec import only works on empty logins tables.
    #[error("The logins tables are not empty")]
    NonEmptyTable,

    #[error("The provided salt is invalid")]
    InvalidSalt,

    #[error("Error synchronizing: {0}")]
    SyncAdapterError(#[from] sync15::Error),

    #[error("Error parsing JSON data: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("Error parsing URL: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("{0}")]
    Interrupted(#[from] interrupt_support::Interrupted),
}

error_support::define_error! {
    ErrorKind {
        (SyncAdapterError, sync15::Error),
        (JsonError, serde_json::Error),
        (UrlParseError, url::ParseError),
        (SqlError, rusqlite::Error),
        (InvalidLogin, InvalidLogin),
        (Interrupted, interrupt_support::Interrupted),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidLogin {
    // EmptyOrigin error occurs when the login's hostname field is empty.
    #[error("Origin is empty")]
    EmptyOrigin,
    #[error("Password is empty")]
    EmptyPassword,
    #[error("Login already exists")]
    DuplicateLogin,
    #[error("Both `formActionUrl` and `httpRealm` are present")]
    BothTargets,
    #[error("Neither `formActionUrl` or `httpRealm` are present")]
    NoTarget,
    #[error("Login has illegal field: {field_info}")]
    IllegalFieldValue { field_info: String },
}

impl Error {
    // Get a short textual label identifying the type of error that occurred,
    // but without including any potentially-sensitive information.
    pub fn label(&self) -> &'static str {
        match self.kind() {
            ErrorKind::BadSyncStatus(_) => "BadSyncStatus",
            ErrorKind::DuplicateGuid(_) => "DuplicateGuid",
            ErrorKind::NoSuchRecord(_) => "NoSuchRecord",
            ErrorKind::NonEmptyTable => "NonEmptyTable",
            ErrorKind::InvalidSalt => "InvalidSalt",
            ErrorKind::SyncAdapterError(_) => "SyncAdapterError",
            ErrorKind::JsonError(_) => "JsonError",
            ErrorKind::UrlParseError(_) => "UrlParseError",
            ErrorKind::SqlError(_) => "SqlError",
            ErrorKind::Interrupted(_) => "Interrupted",
            ErrorKind::InvalidLogin(desc) => match desc {
                InvalidLogin::EmptyOrigin => "InvalidLogin::EmptyOrigin",
                InvalidLogin::EmptyPassword => "InvalidLogin::EmptyPassword",
                InvalidLogin::DuplicateLogin => "InvalidLogin::DuplicateLogin",
                InvalidLogin::BothTargets => "InvalidLogin::BothTargets",
                InvalidLogin::NoTarget => "InvalidLogin::NoTarget",
                InvalidLogin::IllegalFieldValue { .. } => "InvalidLogin::IllegalFieldValue",
            },
        }
    }
}
