use suggest::RemoteSettingsConfig;
use suggest::SuggestIngestionConstraints;
use suggest::SuggestStore;
use suggest::SuggestionProvider;
pub fn main() {
    viaduct_reqwest::use_reqwest_backend();
    let config = RemoteSettingsConfig {
        server: Some(suggest::RemoteSettingsServer::Prod),
        server_url: None,
        bucket_name: None,
        collection_name: "quicksuggest".to_string(),
    };
    let store = SuggestStore::new("./store.db", Some(config)).unwrap();
    let providers = vec![
        SuggestionProvider::Yelp,
        SuggestionProvider::Weather,
        SuggestionProvider::Wikipedia,
        SuggestionProvider::Amo,
        SuggestionProvider::Amp,
    ];
    store
        .ingest(SuggestIngestionConstraints {
            max_suggestions: None,
            providers: Some(providers.clone()),
            empty_only: false,
        })
        .unwrap();
}
