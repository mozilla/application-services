/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::{sync::Arc, time::Duration};

use ads_client::{
    MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest, worker::ErrorOnlyRequestCallback,
};

fn init_backend() {
    viaduct_hyper::viaduct_init_backend_hyper();
}

fn prod_client() -> ads_client::MozAdsClient {
    Arc::new(MozAdsClientBuilder::new())
        .environment(MozAdsEnvironment::Prod)
        .build()
}

struct TestErrorCallback;
impl ErrorOnlyRequestCallback for TestErrorCallback {
    fn on_error(&self,err: ads_client::MozAdsClientApiError) {
        panic!("Error received in background worker callback: {err}")
    }
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_contract_image_prod() {
    init_backend();

    let placement_id= "mock_billboard_1".to_string();
    let client = prod_client();
    let result = client.prefetch_image_ads(vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: placement_id.clone(),
        }],
        None, Some(Box::new(TestErrorCallback)));
        
    assert!(
        result.is_ok(),
        "Image ad dispatch request failed: {:?}",
        result.err()
    );

    // TODO: magic number
    // TODO: good consistent ping
    let ping = client.ping_background_worker(Some(Duration::from_secs(30)), None);
    assert!(
        ping.is_ok(),
        "Ping failed: {:?}",
        ping.err()
    );

    let result = client.query_image_ads(placement_id);
        assert!(
        result.is_ok(),
        "Querying for ads failed: {:?}",
        result.err()
    );
    let placements = result.unwrap();

    assert!(placements.is_some());
}