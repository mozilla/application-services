/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub type Result<T, E = PushError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    /// An unspecified general error has occured
    #[error("General Error: {0:?}")]
    GeneralError(String),

    #[error("Crypto error: {0}")]
    CryptoError(String),

    /// A Client communication error
    #[error("Communication Error: {0:?}")]
    CommunicationError(String),

    /// An error returned from the registration Server
    #[error("Communication Server Error: {0}")]
    CommunicationServerError(String),

    /// Channel is already registered, generate new channelID
    #[error("Channel already registered.")]
    AlreadyRegisteredError,

    /// An error with Storage
    #[error("Storage Error: {0:?}")]
    StorageError(String),

    #[error("No record for chid {0:?}")]
    RecordNotFoundError(String),

    /// A failure to encode data to/from storage.
    #[error("Error executing SQL: {0}")]
    StorageSqlError(#[from] rusqlite::Error),

    #[error("Missing Registration Token")]
    MissingRegistrationTokenError,

    #[error("Transcoding Error: {0}")]
    TranscodingError(String),

    /// A failure to parse a URL.
    #[error("URL parse error: {0:?}")]
    UrlParseError(#[from] url::ParseError),

    /// A failure deserializing json.
    #[error("Failed to parse json: {0}")]
    JSONDeserializeError(#[from] serde_json::Error),

    /// The UAID was not recognized by the server
    #[error("Unrecognized UAID: {0}")]
    UAIDNotRecognizedError(String),

    /// Was unable to send request to server
    #[error("Unable to send request to server: {0}")]
    RequestError(#[from] viaduct::Error),

    #[error("Error opening database: {0}")]
    OpenDatabaseError(#[from] sql_support::open_database::Error),
}
