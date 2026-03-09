/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::hash::{Hash, Hasher};
use std::time::Duration;

use ads_client::{
    http_cache::{ByteSize, CacheOutcome, HttpCache, RequestCachePolicy},
    MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount,
};
use std::sync::Arc;
use url::Url;
use viaduct::Request;

/// Test-only hashable wrapper around Request.
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

// ── Contract tests against the MARS staging server ────────────────────────────
//
// These tests validate that our Rust types can round-trip real responses from
// the MARS staging environment (ads.allizom.org). They are #[ignore] by default
// and should be run:
//   - manually:  cargo test -p ads-client --test integration_test -- --ignored
//   - in CI:     a dedicated Taskcluster task gated on components/ads-client/** changes
//
// If a test fails it means either our types have drifted from the MARS schema
// or the staging server is returning unexpected data — both are worth investigating.

/// Build a client pointed at the MARS staging server.
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

    let ad = placements
        .get("mock_pocket_billboard_1")
        .expect("Placement should exist");

    // Assert all required spec fields are present and non-empty
    assert!(!ad.block_key.is_empty(), "block_key should be non-empty");
    assert!(!ad.format.is_empty(), "format should be non-empty");
    assert!(!ad.image_url.is_empty(), "image_url should be non-empty");
    assert!(!ad.url.is_empty(), "url should be non-empty");
    assert!(
        !ad.callbacks.click.as_str().is_empty(),
        "callbacks.click should be non-empty"
    );
    assert!(
        !ad.callbacks.impression.as_str().is_empty(),
        "callbacks.impression should be non-empty"
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

    let spocs = placements.get("newtab_spocs").expect("Placement should exist");
    assert!(!spocs.is_empty(), "Should have received at least one spoc");

    let ad = &spocs[0];
    assert!(!ad.block_key.is_empty(), "block_key should be non-empty");
    assert!(!ad.format.is_empty(), "format should be non-empty");
    assert!(!ad.image_url.is_empty(), "image_url should be non-empty");
    assert!(!ad.url.is_empty(), "url should be non-empty");
    assert!(!ad.title.is_empty(), "title should be non-empty");
    assert!(!ad.domain.is_empty(), "domain should be non-empty");
    assert!(!ad.excerpt.is_empty(), "excerpt should be non-empty");
    assert!(!ad.sponsor.is_empty(), "sponsor should be non-empty");
    assert!(!ad.caps.cap_key.is_empty(), "caps.cap_key should be non-empty");
    assert!(
        !ad.callbacks.click.as_str().is_empty(),
        "callbacks.click should be non-empty"
    );
    assert!(
        !ad.callbacks.impression.as_str().is_empty(),
        "callbacks.impression should be non-empty"
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

    let ad = placements
        .get("newtab_tile_1")
        .expect("Placement should exist");

    assert!(!ad.block_key.is_empty(), "block_key should be non-empty");
    assert!(!ad.format.is_empty(), "format should be non-empty");
    assert!(!ad.image_url.is_empty(), "image_url should be non-empty");
    assert!(!ad.url.is_empty(), "url should be non-empty");
    assert!(!ad.name.is_empty(), "name should be non-empty");
    assert!(
        !ad.callbacks.click.as_str().is_empty(),
        "callbacks.click should be non-empty"
    );
    assert!(
        !ad.callbacks.impression.as_str().is_empty(),
        "callbacks.impression should be non-empty"
    );
}

// ── Prod tests (existing) ──────────────────────────────────────────────────────

#[test]
#[ignore]
fn test_mock_pocket_billboard_1_placement() {
    viaduct_dev::init_backend_dev();

    let client = MozAdsClientBuilder::new().build();

    let placement_request = MozAdsPlacementRequest {
        placement_id: "mock_pocket_billboard_1".to_string(),
        iab_content: None,
    };

    let result = client.request_image_ads(vec![placement_request], None);

    assert!(result.is_ok(), "Failed to request ads: {:?}", result.err());

    let placements = result.unwrap();

    assert!(
        placements.contains_key("mock_pocket_billboard_1"),
        "Response should contain placement_id 'mock_pocket_billboard_1'"
    );

    placements
        .get("mock_pocket_billboard_1")
        .expect("Placement should exist");
}

#[test]
#[ignore]
fn test_newtab_spocs_placement() {
    viaduct_dev::init_backend_dev();

    let client = MozAdsClientBuilder::new().build();

    let count = 3;
    let placement_request = MozAdsPlacementRequestWithCount {
        placement_id: "newtab_spocs".to_string(),
        count,
        iab_content: None,
    };

    let result = client.request_spoc_ads(vec![placement_request], None);

    assert!(result.is_ok(), "Failed to request ads: {:?}", result.err());

    let placements = result.unwrap();

    assert!(
        placements.contains_key("newtab_spocs"),
        "Response should contain placement_id 'newtab_spocs'"
    );

    let spocs = placements
        .get("newtab_spocs")
        .expect("Placement should exist");

    assert_eq!(
        spocs.len(),
        count as usize,
        "Number of spocs should equal count parameter"
    );
}

#[test]
#[ignore]
fn test_newtab_tile_1_placement() {
    viaduct_dev::init_backend_dev();

    let client = MozAdsClientBuilder::new().build();

    let placement_request = MozAdsPlacementRequest {
        placement_id: "newtab_tile_1".to_string(),
        iab_content: None,
    };

    let result = client.request_tile_ads(vec![placement_request], None);

    assert!(result.is_ok(), "Failed to request ads: {:?}", result.err());

    let placements = result.unwrap();

    assert!(
        placements.contains_key("newtab_tile_1"),
        "Response should contain placement_id 'newtab_tile_1'"
    );

    placements
        .get("newtab_tile_1")
        .expect("Placement should exist");
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
