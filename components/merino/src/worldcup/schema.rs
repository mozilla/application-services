use uniffi::Record;

#[derive(Clone, Debug, Record)]
pub struct WorldCupConfig {
    pub base_host: Option<String>,
}

/// Options for world cup endpoint requests.
/// All fields are optional — omitted fields are not sent to merino.
#[derive(Clone, Debug, Record)]
pub struct WorldCupOptions {
    /// Maximum number of results to return.
    pub limit: Option<u32>,
    /// Filter results by team(s) (e.g. `["FRA", "ENG"]`).
    pub teams: Option<Vec<String>>,
    /// Language for results (e.g. `"en-US"`). (Not supported yet)
    pub accept_language: Option<String>,
}
