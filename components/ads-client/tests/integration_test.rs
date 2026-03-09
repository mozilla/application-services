/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::hash::{Hash, Hasher};
use std::time::Duration;

use ads_client::{
    http_cache::{ByteSize, CacheOutcome, HttpCache, RequestCachePolicy},
    MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest,
    MozAdsPlacementRequestWithCount,
};
use std::sync::Arc;
use url::Url;
use viaduct::Request;

#[derive(Clone)]
struct TestRequest(Request);

impl Hash for TestRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.method.as_str().hash(state);
        self.0.url.as_str().hash(state);
    }
}

impl From<TestRequest> for Request {
    fn from(t: TestRequest) -> Self {
        t.0
    }
}

/// Contract tests against the MARS staging server (ads.allizom.org).
/// Run with: cargo test -p ads-client --test integration_test -- --ignored
fn staging_client() -> ads_client::MozAdsClient {
    Arc::new(MozAdsClientBuilder::new())
        .environment(MozAdsEnvironment::Staging)
        .build()
}

#[test]
#[ignore = "contract test: run manually or in dedicated CI against ads.allizom.org"]
fn test_contract_image_staging() {
    viaduct_dev::init_backend_dev();

    let client = staging_client();
    let result = client.request_image_ads(
        vec![MozAdsPlacementRequest {
            placement_id: "mock_pocket_billboard_1".to_string(),
            iab_content: None,
        }],
        None,
    );

    assert!(result.is_ok(), "Image ad request failed: {:?}", result.err());
    let placements = result.unwrap();
    assert!(
        placements.contains_key("mock_pocket_billboard_1"),
        "Response missing expected placement key"
    );
}

#[test]
#[ignore = "contract test: run manually or in dedicated CI against ads.allizom.org"]
fn test_contract_spoc_staging() {
    viaduct_dev::init_backend_dev();

    let client = staging_client();
    let result = client.request_spoc_ads(
        vec![MozAdsPlacementRequestWithCount {
            placement_id: "newtab_spocs".to_string(),
            count: 1,
            iab_content: None,
        }],
        None,
    );

    assert!(result.is_ok(), "Spoc ad request failed: {:?}", result.err());
    let placements = result.unwrap();
    assert!(
        placements.contains_key("newtab_spocs"),
        "Response missing expected placement key"
    );
}

#[test]
#[ignore = "contract test: run manually or in dedicated CI against ads.allizom.org"]
fn test_contract_tile_staging() {
    viaduct_dev::init_backend_dev();

    let client = staging_client();
    let result = client.request_tile_ads(
        vec![MozAdsPlacementRequest {
            placement_id: "newtab_tile_1".to_string(),
            iab_content: None,
        }],
        None,
    );

    assert!(result.is_ok(), "Tile ad request failed: {:?}", result.err());
    let placements = result.unwrap();
    assert!(
        placements.contains_key("newtab_tile_1"),
        "Response missing expected placement key"
    );
}

#[test]
#[ignore]
fn test_cache_works_using_real_timeouts() {
    viaduct_dev::init_backend_dev();

    let cache: HttpCache<TestRequest> = HttpCache::builder("integration_tests.db")
        .default_ttl(Duration::from_secs(60))
        .max_size(ByteSize::mib(1))
        .build()
        .expect("cache build should succeed");

    let url = Url::parse("https://ads.mozilla.org/v1/ads").unwrap();
    let req = TestRequest(Request::post(url).json(&serde_json::json!({
        "context_id": "12347fff-00b0-aaaa-0978-189231239808",
        "placements": [
            {
            "placement": "mock_pocket_billboard_1",
            "count": 1,
            }
        ],
    })));

    let test_ttl = 2;

    // First call: miss -> store
    let o1 = cache
        .send_with_policy(
            req.clone(),
            &RequestCachePolicy {
                mode: ads_client::http_cache::CacheMode::CacheFirst,
                ttl_seconds: Some(test_ttl),
            },
        )
        .unwrap();
    matches!(o1.cache_outcome, CacheOutcome::MissStored);

    // Second call: hit (no extra HTTP) but no refresh
    let o2 = cache
        .send_with_policy(req.clone(), &RequestCachePolicy::default())
        .unwrap();
    matches!(o2.cache_outcome, CacheOutcome::Hit);
    assert_eq!(o2.response.status, 200);

    // Third call: Miss due to timeout for the test_ttl duration
    std::thread::sleep(Duration::from_secs(test_ttl));
    let o3 = cache
        .send_with_policy(req, &RequestCachePolicy::default())
        .unwrap();
    matches!(o3.cache_outcome, CacheOutcome::MissStored);
    assert_eq!(o3.response.status, 200);
}
