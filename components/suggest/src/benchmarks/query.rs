/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    benchmarks::{new_store, BenchmarkWithInput},
    SuggestStore, SuggestionProvider, SuggestionQuery,
};

pub struct QueryBenchmark {
    provider: SuggestionProvider,
    query: &'static str,
}

impl QueryBenchmark {
    pub fn new(provider: SuggestionProvider, query: &'static str) -> Self {
        Self { provider, query }
    }
}

impl BenchmarkWithInput for QueryBenchmark {
    type GlobalInput = SuggestStore;
    type IterationInput = SuggestionQuery;

    fn global_input(&self) -> Self::GlobalInput {
        new_store()
    }

    fn iteration_input(&self) -> Self::IterationInput {
        SuggestionQuery {
            providers: vec![self.provider],
            keyword: self.query.to_string(),
            ..SuggestionQuery::default()
        }
    }

    fn benchmarked_code(&self, store: &Self::GlobalInput, query: Self::IterationInput) {
        store
            .query(query)
            .unwrap_or_else(|e| panic!("Error querying store: {e}"));
    }
}

pub fn all_benchmarks() -> Vec<(&'static str, QueryBenchmark)> {
    vec![
        // Fakespot queries, these attempt to perform prefix matches with various character
        // lengths.
        //
        // The query code will only do a prefix match if the total input length is > 3 chars.
        // Therefore, to test shorter prefixes we use 2-term queries.
        (
            "query-fakespot-hand-s",
            QueryBenchmark::new(SuggestionProvider::Fakespot, "hand s"),
        ),
        (
            "query-fakespot-hand-sa",
            QueryBenchmark::new(SuggestionProvider::Fakespot, "hand sa"),
        ),
        (
            "query-fakespot-hand-san",
            QueryBenchmark::new(SuggestionProvider::Fakespot, "hand san"),
        ),
        (
            "query-fakespot-sani",
            QueryBenchmark::new(SuggestionProvider::Fakespot, "sani"),
        ),
        (
            "query-fakespot-sanit",
            QueryBenchmark::new(SuggestionProvider::Fakespot, "sanit"),
        ),
        (
            "query-fakespot-saniti",
            QueryBenchmark::new(SuggestionProvider::Fakespot, "saniti"),
        ),
    ]
}
