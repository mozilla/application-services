/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module implements the sampling logic required to hash,
//! randomize and pick branches using pre-set ratios.

use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use std::convert::TryInto;

const HASH_BITS: u32 = 48;
const HASH_LENGTH: u32 = HASH_BITS / 4;

/// Sample by splitting the input space into a series of buckets, checking
/// if the given input is in a range of buckets
///
/// The range to check is defined by a start point and length, and can wrap around
/// the input space. For example, if there are 100 buckets, and we ask to check 50 buckets
/// starting from bucket 70, then buckets 70-99 and 0-19 will be checked
///
/// # Arguments:
///
/// - `input` What will be hashed and matched against the range of the buckets
/// - `start` the index of the bucket to start checking
/// - `count` then number of buckets to check
/// - `total` The total number of buckets to group inputs into
///
/// # Returns:
///
/// Returns true if the hash generated from the input belongs within the range
/// otherwise false
///
/// # Errors:
///
/// Could error in the following cases (but not limited to)
/// - An error occured in the hashing process
/// - an error occured while checking if the hash belongs in the bucket
pub(crate) fn bucket_sample<T: serde::Serialize>(
    input: T,
    start: u32,
    count: u32,
    total: u32,
) -> Result<bool> {
    let input_hash = hex::encode(truncated_hash(input)?);
    let wrapped_start = start % total;
    let end = wrapped_start + count;

    Ok(if end > total {
        is_hash_in_bucket(&input_hash, 0, end % total, total)?
            || is_hash_in_bucket(&input_hash, wrapped_start, total, total)?
    } else {
        is_hash_in_bucket(&input_hash, wrapped_start, end, total)?
    })
}

/// Sample over a list of ratios such that, over the input space, each
/// ratio has a number of matches in correct proportion to the other ratios
///
/// # Arguments:
/// - `input`: the input used in the sampling process
/// - `ratios`: The list of ratios associated with each option
///
/// # Example:
///
/// Assuming the ratios: `[1, 2, 3, 4]`
/// 10% of all inputs will return 0, 20% will return 1 and so on
///
/// # Returns
/// Returns an index of the ratio that matched the input
///
/// # Errors
/// Could return an error if the input couldn't be hashed
pub(crate) fn ratio_sample<T: serde::Serialize>(input: T, ratios: &[u32]) -> Result<usize> {
    if ratios.is_empty() {
        return Err(Error::EmptyRatiosError);
    }
    let input_hash = hex::encode(truncated_hash(input)?);
    let ratio_total = ratios.iter().fold(0u32, |acc, r| acc + r);
    let mut sample_point = 0;
    for i in 0..ratios.len() - 1 {
        sample_point += ratios[i];
        if input_hash <= fraction_to_key(sample_point as f64 / ratio_total as f64)? {
            return Ok(i);
        }
    }
    Ok(ratios.len() - 1)
}

/// Provides a hash of `data`, truncated to the 6 most significant bytes
/// For consistency with: https://searchfox.org/mozilla-central/source/toolkit/components/utils/Sampling.jsm#79
/// # Arguments:
/// - `data`: The data to be hashed
///
/// # Returns:
/// Returns the 6 bytes associted with the SHA-256 of the data
///
/// # Errors:
/// Would return an error if the hashing function fails to generate a hash
/// that is larger than 6 bytes (Should never occur)
fn truncated_hash<T: serde::Serialize>(data: T) -> Result<[u8; 6]> {
    let mut hasher = Sha256::new();
    let data_str = serde_json::to_string(&data)?;
    hasher.update(data_str.as_bytes());
    Ok(hasher.finalize()[0..6].try_into()?)
}

/// Checks if a given hash (represented as a 6 byte hex string) fits within a bucket range
///
/// # Arguments:
/// - `input_hash_num`: The hash as a 6 byte hex string (12 hex digits)
/// - `min_bucket`: The minimum bucket number
/// - `max_bucket`: The maximum bucket number
/// - `bucket_count`: The number of buckets
///
/// # Returns
/// Returns true if the has fits in the bucket range,
/// otherwise false
///
/// # Errors:
///
/// Could return an error if bucket numbers are higher than the bucket count
fn is_hash_in_bucket(
    input_hash_num: &str,
    min_bucket: u32,
    max_bucket: u32,
    bucket_count: u32,
) -> Result<bool> {
    let min_hash = fraction_to_key(min_bucket as f64 / bucket_count as f64)?;
    let max_hash = fraction_to_key(max_bucket as f64 / bucket_count as f64)?;
    Ok(min_hash.as_str() <= input_hash_num && input_hash_num < max_hash.as_str())
}

/// Maps from the range [0, 1] to [0, 2^48]
///
/// # Argument:
/// - `fraction`: float in the range 0-1
///
/// # Returns
/// returns a hex string representing the fraction multiplied to be within the
/// [0, 2^48] range
///
/// # Errors
/// returns an error if the fraction not within the 0-1 range
fn fraction_to_key(fraction: f64) -> Result<String> {
    if fraction < 0.0 || fraction > 1.0 {
        return Err(Error::InvalidFraction);
    }
    let multiplied = (fraction * (2u64.pow(HASH_BITS) - 1) as f64).floor();
    let multiplied = format!("{:x}", multiplied as u64);
    let padding = vec!['0'; HASH_LENGTH as usize - multiplied.len()];
    let res = padding
        .into_iter()
        .chain(multiplied.chars())
        .collect::<String>();
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Error::EmptyRatiosError => (), // okay,
            _ => panic!("Should be an empty ratios error!"),
        }
    }

    #[test]
    fn test_bucket_sample() {
        // Different combinations here tested against the
        // deskop implementation
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
}
