/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */
use crate::metrics::{EnrollmentStatusExtraDef, MetricsHandler};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A Rust implementation of the MetricsHandler trait
/// Used to test recording of Glean metrics across the FFI within Rust
///
/// *NOTE: Use this struct's `new` method when instantiating it to lock the Glean store*
#[derive(Clone)]
pub struct TestMetrics {
    state: Arc<Mutex<HashMap<String, serde_json::Value>>>,
}

impl TestMetrics {
    pub fn new() -> Self {
        TestMetrics {
            state: Default::default(),
        }
    }

    pub fn assert_get_vec_value(&self, key: &str) -> serde_json::Value {
        self.state.lock().unwrap().get(key).unwrap().clone()
    }
}

impl MetricsHandler for TestMetrics {
    fn record_enrollment_statuses(&self, enrollment_status_extras: Vec<EnrollmentStatusExtraDef>) {
        let key = "enrollment_status";
        let mut state = self.state.lock().unwrap();
        let new = serde_json::to_value(enrollment_status_extras).unwrap();
        match state.get(key) {
            Some(v) => {
                let new_value = v
                    .as_array()
                    .unwrap()
                    .iter()
                    .chain(new.as_array().unwrap())
                    .cloned()
                    .collect::<Vec<serde_json::Value>>();
                state.insert(key.into(), new_value.into());
            }
            None => {
                state.insert(key.into(), new);
            }
        };
    }
}
