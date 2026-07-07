/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, HashSet};

use crate::{
    impression_log::{ImpressionLog, ImpressionLogOutcome},
    mars::ad_response::{AdResponse, AdResponseValue},
    telemetry::Telemetry,
    ImpressionCappingPolicy,
};

pub struct MARSCapping<T: Telemetry> {
    impression_log: Option<ImpressionLog>,
    telemetry: T,
}

impl<T: Telemetry> MARSCapping<T> {
    pub fn new(impression_log: Option<ImpressionLog>, telemetry: T) -> Self {
        Self {
            impression_log,
            telemetry,
        }
    }

    pub fn record_impression(&self, cap_key: &str) {
        if let Some(impression_log) = &self.impression_log {
            if let Err(e) = impression_log.record_impression(cap_key) {
                self.telemetry
                    .record(&ImpressionLogOutcome::RecordImpressionFailed(e));
            }
        };
    }

    pub fn apply_impression_capping<A: AdResponseValue>(
        &self,
        mut ads: AdResponse<A>,
        impression_capping_policy: &ImpressionCappingPolicy,
    ) -> AdResponse<A> {
        if let Some(impression_log) = &self.impression_log {
            let caps: HashMap<&str, &u32> = ads
                .data
                .iter()
                .flat_map(|(_, placement_ads)| placement_ads.iter().flat_map(|a| a.cap_pair()))
                .collect();

            let counts = match impression_log.count_impressions(caps.keys()) {
                Ok(counts) => counts,
                Err(e) => {
                    self.telemetry
                        .record(&ImpressionLogOutcome::CountImpressionsFailed(e));

                    // Skip unnecessary work if DB access failed
                    return ads;
                }
            };

            if let Err(e) = impression_log.retain_impressions(caps.keys()) {
                self.telemetry
                    .record(&ImpressionLogOutcome::RetainImpressionsFailed(e));
            };

            let cap_keys_to_filter: HashSet<String> = caps
                .iter()
                .flat_map(|(&cap_key, max_impressions)| {
                    if counts.get(cap_key).unwrap_or(&0) >= max_impressions {
                        self.telemetry
                            .record(&ImpressionLogOutcome::ImpressionCapHit);
                        match impression_capping_policy {
                            ImpressionCappingPolicy::TelemetryOnly => {
                                self.telemetry
                                    .record(&ImpressionLogOutcome::ImpressionCapNotEnforced);
                                None
                            }
                            ImpressionCappingPolicy::ImpressionCapEnforced => {
                                self.telemetry
                                    .record(&ImpressionLogOutcome::ImpressionCapEnforced);
                                Some(cap_key.to_owned())
                            }
                        }
                    } else {
                        None
                    }
                })
                .collect();

            if !cap_keys_to_filter.is_empty() {
                ads.data.iter_mut().for_each(|(_, placement_ads)| {
                    placement_ads.retain(|a| {
                        if let Some((cap_key, _)) = a.cap_pair() {
                            !cap_keys_to_filter.contains(cap_key)
                        } else {
                            true
                        }
                    });
                });
            }
        };

        ads
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use crate::ffi::telemetry::MozAdsTelemetryWrapper;
    use crate::impression_log::ImpressionCappingPolicy;
    use crate::mars::ad_response::{AdCallbacks, AdSpoc, SpocFrequencyCaps, SpocRanking};

    use super::*;

    #[test]
    fn test_no_impression_log_does_not_error() {
        let capping = MARSCapping::new(None, MozAdsTelemetryWrapper::noop());

        let spoc = AdSpoc {
            block_key: "test_block_key".into(),
            callbacks: AdCallbacks {
                click: Url::parse("https://example.com/test_click").unwrap(),
                impression: Url::parse("https://example.com/test_impression").unwrap(),
                report: None,
            },
            caps: SpocFrequencyCaps {
                cap_key: "test_cap_key".into(),
                day: 10,
            },
            domain: "example.com".into(),
            excerpt: "test_excerpt".into(),
            format: "test_format".into(),
            image_url: "https://example.com/test_image".into(),
            ranking: SpocRanking {
                priority: 0,
                personalization_models: None,
                item_score: 0.0,
            },
            sponsor: "test_sponsor".into(),
            sponsored_by_override: None,
            title: "test_title".into(),
            url: "https://example.com/test_url".into(),
        };

        capping.record_impression("test_cap_key");
        capping.apply_impression_capping(
            AdResponse::<AdSpoc> {
                data: HashMap::from([("".into(), vec![spoc.clone()])]),
            },
            &ImpressionCappingPolicy::TelemetryOnly,
        );
        capping.apply_impression_capping(
            AdResponse::<AdSpoc> {
                data: HashMap::from([("".into(), vec![spoc.clone()])]),
            },
            &ImpressionCappingPolicy::TelemetryOnly,
        );
    }
}
