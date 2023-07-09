use std::sync::{Arc, Mutex};

use crate::{store::SuggestStore, SuggestApiError, Suggestion};

#[derive(Clone, Debug, Default)]
struct QueryState {
    keyword: String,
    include_sponsored: bool,
    include_non_sponsored: bool,
}

pub struct SuggestionQuery {
    store: Arc<SuggestStore>,
    state: Mutex<QueryState>,
}

impl SuggestionQuery {
    pub(crate) fn with_store(store: Arc<SuggestStore>) -> Self {
        Self {
            store,
            state: Mutex::default(),
        }
    }

    /// Sets the query to only match suggestions with the given `keyword`.
    pub fn keyword(self: Arc<Self>, keyword: &str) -> Arc<Self> {
        self.state.lock().unwrap().keyword = keyword.into();
        self
    }

    /// Sets the query to return sponsored suggestions.
    pub fn include_sponsored(self: Arc<Self>, include_sponsored: bool) -> Arc<Self> {
        self.state.lock().unwrap().include_sponsored = include_sponsored;
        self
    }

    /// Sets the query to return non-sponsored suggestions.
    pub fn include_non_sponsored(self: Arc<Self>, include_non_sponsored: bool) -> Arc<Self> {
        self.state.lock().unwrap().include_non_sponsored = include_non_sponsored;
        self
    }

    /// Returns matching suggestions for the query.
    pub fn results(&self) -> Result<Vec<Suggestion>, SuggestApiError> {
        let state = self.state.lock().unwrap();
        if state.keyword.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .store
            .dbs()?
            .reader
            .fetch_by_keyword(&state.keyword)?
            .into_iter()
            .filter(|suggestion| {
                (suggestion.is_sponsored && state.include_sponsored)
                    || (!suggestion.is_sponsored && state.include_non_sponsored)
            })
            .collect())
    }
}
