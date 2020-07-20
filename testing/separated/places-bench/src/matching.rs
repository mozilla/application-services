/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use criterion::Criterion;
use places::match_impl::{AutocompleteMatch, MatchBehavior, SearchBehavior};

pub fn bench_match_anywhere(c: &mut Criterion) {
    c.bench_function("match anywhere url", |b| {
        let matcher = AutocompleteMatch {
            search_str: "lication-servic",
            url_str: "https://github.com/mozilla/application-services/",
            title_str: "mozilla/application-services: Firefox Application Services",
            tags: "",
            visit_count: 100,
            typed: false,
            bookmarked: false,
            open_page_count: 0,
            match_behavior: MatchBehavior::Anywhere,
            search_behavior: SearchBehavior::default(),
        };
        b.iter(|| matcher.invoke())
    });
    c.bench_function("match anywhere title casecmp", |b| {
        let matcher = AutocompleteMatch {
            search_str: "notpresent services",
            url_str: "https://github.com/mozilla/application-services/",
            title_str: "mozilla/application-services: Firefox Application Services",
            tags: "",
            match_behavior: MatchBehavior::Anywhere,
            visit_count: 100,
            typed: false,
            bookmarked: false,
            open_page_count: 0,
            search_behavior: SearchBehavior::default(),
        };
        b.iter(|| matcher.invoke())
    });
}
