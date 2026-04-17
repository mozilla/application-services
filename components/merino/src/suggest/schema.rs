use uniffi::Record;

/// Configuration for the merino suggest client.
#[derive(Clone, Debug, Record)]
pub struct SuggestConfig {
    /// The base host for the merino endpoint. Defaults to the production host if not set.
    pub base_host: Option<String>,
}

/// Options for a suggest request, mapped to merino suggest endpoint query parameters.
/// All fields are optional — omitted fields are not sent to merino.
#[derive(Clone, Debug, Record)]
pub struct SuggestOptions {
    /// List of suggestion providers to query (e.g. `["wikipedia", "adm"]`).
    pub providers: Option<Vec<String>>,
    /// Identifier of which part of firefox the request comes from (e.g. `"urlbar"`, `"newtab"`).
    pub source: Option<String>,
    /// ISO 3166-1 country code (e.g. `"US"`).
    pub country: Option<String>,
    /// Comma separated string of subdivision code(s) (e.g. `"CA"`).
    pub region: Option<String>,
    /// City name (e.g. `"San Francisco"`).
    pub city: Option<String>,
    /// List of any experiments or rollouts that are affecting the client's Suggest experience.
    /// If Merino recognizes any of them it will modify its behavior accordingly.
    pub client_variants: Option<Vec<String>>,
    /// For AccuWeather provider, the request type should be either a "location" or "weather" string. For "location" it will get location completion suggestion. For "weather" it will return weather suggestions.
    /// If omitted, it defaults to weather suggestions.
    pub request_type: Option<String>,
    /// The `Accept-Language` header value to forward to Merino (e.g. `"en-US"`).
    pub accept_language: Option<String>,
}
