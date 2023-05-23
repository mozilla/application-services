// Errors we return via the public interface.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error opening database: {0}")]
    OpenDatabase(#[from] sql_support::open_database::Error),

    #[error("Error executing SQL: {0}")]
    Sql(#[from] rusqlite::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Error from Remote Settings: {0}")]
    RemoteSettings(#[from] remote_settings::RemoteSettingsError),

    #[error("Operation interrupted")]
    Interrupted(#[from] interrupt_support::Interrupted),
}

#[derive(Debug, thiserror::Error)]
pub enum SuggestError {
    #[error("Other error: {reason}")]
    Other { reason: String },
}

impl From<Error> for SuggestError {
    fn from(error: Error) -> Self {
        Self::Other {
            reason: error.to_string(),
        }
    }
}
