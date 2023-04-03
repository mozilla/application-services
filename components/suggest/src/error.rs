// Errors we return via the public interface.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error opening database: {0}")]
    OpenDatabaseError(#[from] sql_support::open_database::Error),

    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Error from Remote Settings: {0}")]
    RemoteSettingsError(#[from] remote_settings::RemoteSettingsError),

    #[error("Operation interrupted")]
    InterruptedError(#[from] interrupt_support::Interrupted),
}
