/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

pub mod feeds;
pub mod layout;
pub mod locale;
pub mod request;
pub mod response;

// Re-export all model types for use by UniFFI bindings and downstream consumers.
#[allow(unused_imports)]
pub use feeds::{
    CuratedRecommendationsBucket, FakespotCta, FakespotFeed, FakespotProduct, FeedSection, Feeds,
};
#[allow(unused_imports)]
pub use layout::{Layout, ResponsiveLayout, Tile};
pub use locale::CuratedRecommendationLocale;
#[allow(unused_imports)]
pub use request::{CuratedRecommendationsConfig, CuratedRecommendationsRequest, SectionSettings};
#[allow(unused_imports)]
pub use response::{
    CuratedRecommendationsResponse, InterestPicker, InterestPickerSection, RecommendationDataItem,
};
