/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::sampling::*;
use crate::NimbusError;

#[test]
fn test_truncated_hash() {
    let data = serde_json::json!([1234, "test_namespace"]);
    let truncated_hash = truncated_hash(data).unwrap();
    // Test vector retrieved from testing the same input against
    // crypto.subtle in js
    assert_eq!(hex::encode(truncated_hash), "2a6e18a86edc")
}

#[test]
fn test_truncated_hash_null_data() {
    let data = serde_json::json!(null);
    let truncated_hash = truncated_hash(data).unwrap();
    // Test vector retrieved from testing the same input against
    // crypto.subtle in js
    assert_eq!(hex::encode(truncated_hash), "74234e98afe7")
}

#[test]
fn test_truncated_hash_empty_string_data() {
    let data = serde_json::json!("");
    let truncated_hash = truncated_hash(data).unwrap();
    // Test vector retrieved from testing the same input against
    // crypto.subtle in js
    assert_eq!(hex::encode(truncated_hash), "12ae32cb1ec0")
}

#[test]
fn test_truncated_hash_object_data() {
    let data = serde_json::json!({"id": 1234, "namespace": "experiment"});
    let truncated_hash = truncated_hash(data).unwrap();
    // Test vector retrieved from testing the same input against
    // crypto.subtle in js
    assert_eq!(hex::encode(truncated_hash), "5d1effd4b032")
}

#[test]
fn test_ratio_sample() {
    let input = format!(
        "experiment-manager-{:}-{:}-branch",
        "299eed1e-be6d-457d-9e53-da7b1a03f10d", "TEST_EXP1"
    );
    let ratios = vec![1, 1];
    // 299eed1e-be6d-457d-9e53-da7b1a03f10d matches against the second index (index = 1)
    // tested against the desktop implementation
    assert_eq!(ratio_sample(input, &ratios).unwrap(), 1);
    let input = format!(
        "experiment-manager-{:}-{:}-branch",
        "542213c0-9aef-47eb-bc6b-3b8529736ba2", "TEST_EXP1"
    );
    // 542213c0-9aef-47eb-bc6b-3b8529736ba2 matches against the first index (index = 0)
    // tested against the desktop implementation
    assert_eq!(ratio_sample(input, &ratios).unwrap(), 0);
}

#[test]
fn test_empty_ratios() {
    let input = "does not matter";
    let ratios = Vec::new();
    let res = ratio_sample(input, &ratios);
    match res.unwrap_err() {
        NimbusError::EmptyRatiosError => (), // okay,
        _ => panic!("Should be an empty ratios error!"),
    }
}

#[test]
fn test_bucket_sample() {
    // Different combinations here tested against the
    // desktop implementation
    let input = serde_json::json!([
        "299eed1e-be6d-457d-9e53-da7b1a03f10d",
        "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77"
    ]);
    assert!(bucket_sample(input.clone(), 0, 2000, 10000).unwrap());
    assert!(!bucket_sample(input, 2000, 3000, 10000).unwrap());
    let input = serde_json::json!([
        "c590d3f5-fe9d-4820-97c9-f403535dd306",
        "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77"
    ]);
    assert!(!bucket_sample(input.clone(), 0, 2000, 10000).unwrap());
    assert!(bucket_sample(input, 2000, 3000, 10000).unwrap());
}
