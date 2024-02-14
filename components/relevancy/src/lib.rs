/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Proposed API for the relevancy component (validation phase)
//!
//! The goal here is to allow us to validate that we can reliably detect user interests from
//! history data, without spending too much time building the API out.  There's some hand-waving
//! towards how we would use this data to rank search results, but we don't need to come to a final
//! decision on that yet.

/// List of possible interests for a domain.  Each domain would have exactly one of these.  If we
/// can't calculate an interest based on the classifier data, then we put it in `Inconclusive`
#[derive(uniffi::Enum)]
pub enum Interest {
    Animals,
    Arts,
    Autos,
    Business,
    Career,
    Education,
    Fashion,
    Finance,
    Food,
    Government,
    Health,
    Hobbies,
    Home,
    News,
    RealEstate,
    Society,
    Sports,
    Tech,
    Travel,
    Inconclusive,
}

/// An interest combined with a relevancy score
///
/// Scores will be [0-10].  Consumers would use this score to alter the order of their suggestions.
///
/// For example, consumers may assign a default score of 5 to sponsored suggestions and 0 to
/// Wikipedia suggestions.  By default, the sponsored suggestions will show up over Wikipedia
/// suggestions, but if a Wikipedia suggestion had a interest that was highly relevant for the
/// user, that suggestion would show up first instead.
#[derive(uniffi::Record)]
pub struct InterestScore {
    interest: Interest,
    score: u8,
}

/// Top-level class that ingests and stores relevancy data.
///
/// For the validation phase:
///   - All data will be stored in memory.
///   - Consumers will call `ingest` then `get_top_categories` and store the results in a Glean
///     distribution (see `metrics.yaml).  Our main focus will be to monitor these metrics in order
///     to determine if the relevancy scores would be strong enough to affect search rankings.
///   - If possible, I'd also like to have make "about::relevancy" where users can go to see the
///     results themselves.  This way we can pair the Glean metrics with asking people if they
///     think the interest rankings seem correct to them.
#[derive(uniffi::Object)]
pub struct RelavancyStore {
}

/// Glean metrics for the initial validation phase
///
/// See `metrics.yaml` for a description on these.
#[derive(uniffi::Record)]
pub struct InterestMetrics {
  interest_score_max: u16,
  interest_score_max_single_topic: u16
  interest_score_non_inconclusive: u16
}

#[uniffi::export]
impl RelavancyStore {
    /// Ingest relevancy data from remote settings
    ///
    /// This will download the classifier data from remote settings, then process the top N domains
    /// by frecency against that data.  Each domain will be classified into 1 interest, which may be
    /// `Interest::Inconclusive`.
    ///
    /// Based on that, we will build a interest vector.  There will be one dimension for each
    /// interest where the value is the number of domains that were classified with that interest.
    ///
    /// Consumers are responsible for figuring out the top URLs.  I believe this is not hard on
    /// Desktop.  On mobile, it's either currently easy or we could add a function to places that
    /// makes it easy.
    pub fn ingest(&self, top_urls_by_frecency: Vec<String>) {
        todo!()
    }

    /// Calculate Glean metrics values
    ///
    /// The component will calculate the metrics, but it's the consumer's responsibility to handle
    /// the Glean integration.  This probably means the `metrics.yaml` file will end up living in
    /// moz-central.
    pub fn calculate_metrics(&self) -> Vec<InterestMetrics> {
        todo!()
    }
}
