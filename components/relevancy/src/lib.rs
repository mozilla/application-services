/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Proposed API for the relevancy component (validation phase)
//!
//! The goal here is to allow us to validate that we can reliably detect user interests from
//! history data, without spending too much time building the API out.  There's some hand-waving
//! towards how we would use this data to rank search results, but we don't need to come to a final
//! decision on that yet.

mod db;
mod error;
mod ingest;
mod interest;
mod ranker;
mod rs;
mod schema;
pub mod url_hash;

pub use db::RelevancyDb;
pub use error::{ApiResult, Error, RelevancyApiError, Result};
pub use interest::{Interest, InterestVector};
pub use ranker::score;

use error_support::handle_error;

pub struct RelevancyStore {
    db: RelevancyDb,
}

/// Top-level API for the Relevancy component
impl RelevancyStore {
    pub fn new(db_path: String) -> Self {
        Self {
            db: RelevancyDb::new(db_path),
        }
    }

    pub fn close(&self) {
        self.db.close()
    }

    pub fn interrupt(&self) {
        self.db.interrupt()
    }

    /// Download the interest data from remote settings if needed
    #[handle_error(Error)]
    pub fn ensure_interest_data_populated(&self) -> ApiResult<()> {
        ingest::ensure_interest_data_populated(&self.db)?;
        Ok(())
    }

    /// Ingest top URLs to build the user's interest vector.
    ///
    /// Consumer should pass a list of the user's top URLs by frecency to this method.  It will
    /// then:
    ///
    ///  - Download the URL interest data from remote settings.  Eventually this should be cached /
    ///    stored in the database, but for now it would be fine to download fresh data each time.
    ///  - Match the user's top URls against the interest data to build up their interest vector.
    ///  - Store the user's interest vector in the database.
    ///
    ///  This method may execute for a long time and should only be called from a worker thread.
    #[handle_error(Error)]
    pub fn ingest(&self, top_urls_by_frecency: Vec<String>) -> ApiResult<InterestVector> {
        ingest::ensure_interest_data_populated(&self.db)?;
        let interest_vec = self.classify(top_urls_by_frecency)?;
        self.db
            .read_write(|dao| dao.update_frecency_user_interest_vector(&interest_vec))?;
        Ok(interest_vec)
    }

    pub fn classify(&self, top_urls_by_frecency: Vec<String>) -> Result<InterestVector> {
        let mut interest_vector = InterestVector::default();
        for url in top_urls_by_frecency {
            let interest_count = self.db.read(|dao| dao.get_url_interest_vector(&url))?;
            log::trace!("classified: {url} {}", interest_count.summary());
            interest_vector = interest_vector + interest_count;
        }

        Ok(interest_vector)
    }

    /// Calculate metrics for the validation phase
    ///
    /// This runs after [Self::ingest].  It takes the interest vector that ingest created and
    /// calculates a set of metrics that we can report to glean.
    #[handle_error(Error)]
    pub fn calculate_metrics(&self) -> ApiResult<InterestMetrics> {
        todo!()
    }

    /// Get the user's interest vector directly.
    ///
    /// This runs after [Self::ingest].  It returns the interest vector directly so that the
    /// consumer can show it in an `about:` page.
    #[handle_error(Error)]
    pub fn user_interest_vector(&self) -> ApiResult<InterestVector> {
        self.db.read(|dao| dao.get_frecency_user_interest_vector())
    }
}

/// Interest metric data.  See `relevancy.udl` for details.
pub struct InterestMetrics {
    pub top_single_interest_similarity: u32,
    pub top_2interest_similarity: u32,
    pub top_3interest_similarity: u32,
}

uniffi::include_scaffolding!("relevancy");

#[cfg(test)]
mod test {
    use crate::url_hash::hash_url;

    use super::*;

    fn make_fixture() -> Vec<(String, Interest)> {
        vec![
            ("https://food.com/".to_string(), Interest::Food),
            ("https://hello.com".to_string(), Interest::Inconclusive),
            ("https://pasta.com".to_string(), Interest::Food),
            ("https://dog.com".to_string(), Interest::Animals),
        ]
    }

    fn expected_interest_vector() -> InterestVector {
        InterestVector {
            inconclusive: 1,
            animals: 1,
            food: 2,
            ..InterestVector::default()
        }
    }

    fn setup_store(test_id: &'static str) -> RelevancyStore {
        let relevancy_store =
            RelevancyStore::new(format!("file:test_{test_id}_data?mode=memory&cache=shared"));
        relevancy_store
            .db
            .read_write(|dao| {
                for (url, interest) in make_fixture() {
                    dao.add_url_interest(hash_url(&url).unwrap(), interest)?;
                }
                Ok(())
            })
            .expect("Insert should succeed");

        relevancy_store
    }

    #[test]
    fn test_ingest() {
        let relevancy_store = setup_store("ingest");
        let (top_urls, _): (Vec<String>, Vec<Interest>) = make_fixture().into_iter().unzip();

        assert_eq!(
            relevancy_store.ingest(top_urls).unwrap(),
            expected_interest_vector()
        );
    }

    #[test]
    fn test_get_user_interest_vector() {
        let relevancy_store = setup_store("get_user_interest_vector");
        let (top_urls, _): (Vec<String>, Vec<Interest>) = make_fixture().into_iter().unzip();

        relevancy_store
            .ingest(top_urls)
            .expect("Ingest should succeed");

        assert_eq!(
            relevancy_store.user_interest_vector().unwrap(),
            expected_interest_vector()
        );
    }
}
