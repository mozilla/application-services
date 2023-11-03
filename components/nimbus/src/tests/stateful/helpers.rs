/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */
use crate::metrics::{EnrollmentStatusExtraDef, FeatureExposureExtraDef, MetricsHandler};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct MetricState {
    enrollment_statuses: Vec<EnrollmentStatusExtraDef>,
    activations: Vec<FeatureExposureExtraDef>,
}

/// A Rust implementation of the MetricsHandler trait
/// Used to test recording of Glean metrics across the FFI within Rust
///
/// *NOTE: Use this struct's `new` method when instantiating it to lock the Glean store*
#[derive(Clone)]
pub struct TestMetrics {
    state: Arc<Mutex<MetricState>>,
}

impl TestMetrics {
    pub fn new() -> Self {
        TestMetrics {
            state: Default::default(),
        }
    }

    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.activations.clear();
        state.enrollment_statuses.clear();
    }

    pub fn get_enrollment_statuses(&self) -> Vec<EnrollmentStatusExtraDef> {
        self.state.lock().unwrap().enrollment_statuses.clone()
    }

    pub fn get_activations(&self) -> Vec<FeatureExposureExtraDef> {
        self.state.lock().unwrap().activations.clone()
    }
}

impl MetricsHandler for TestMetrics {
    fn record_enrollment_statuses(&self, enrollment_status_extras: Vec<EnrollmentStatusExtraDef>) {
        let mut state = self.state.lock().unwrap();
        state.enrollment_statuses.extend(enrollment_status_extras);
    }

    fn record_feature_activation(&self, event: FeatureExposureExtraDef) {
        let mut state = self.state.lock().unwrap();
        state.activations.push(event);
    }
}
