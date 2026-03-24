/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::hash::{Hash, Hasher};
use std::time::Duration;

use ads_client::http_cache::{ByteSize, CacheMode, CacheOutcome, HttpCache, RequestCachePolicy};
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

fn init_backend() {
    // Err means the backend is already initialized.
    let _ = viaduct_hyper::viaduct_init_backend_hyper();
}

#[test]
fn test_cache_works_using_real_timeouts() {
    init_backend();

    let cache = HttpCache::<TestRequest>::builder("integration_tests.db")
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
                mode: CacheMode::CacheFirst,
                ttl_seconds: Some(test_ttl),
            },
        )
        .unwrap();
    assert!(matches!(o1.cache_outcome, CacheOutcome::MissStored));

    // Second call: hit (no extra HTTP) but no refresh
    let o2 = cache
        .send_with_policy(req.clone(), &RequestCachePolicy::default())
        .unwrap();
    assert!(matches!(o2.cache_outcome, CacheOutcome::Hit));
    assert_eq!(o2.response.status, 200);

    // Third call: Miss due to timeout for the test_ttl duration
    std::thread::sleep(Duration::from_secs(test_ttl));
    let o3 = cache
        .send_with_policy(req, &RequestCachePolicy::default())
        .unwrap();
    assert!(matches!(o3.cache_outcome, CacheOutcome::MissStored));
    assert_eq!(o3.response.status, 200);
}
