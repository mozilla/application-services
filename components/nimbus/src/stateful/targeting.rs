// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    enrollment::ExperimentEnrollment,
    json::JsonObject,
    stateful::behavior::{EventQueryType, EventStore},
    NimbusTargetingHelper, Result, TargetingAttributes,
};
use serde_json::Value;
use serde_json::{json, Map};
use std::sync::{Arc, Mutex};

impl NimbusTargetingHelper {
    pub(crate) fn with_targeting_attributes(
        targeting_attributes: &TargetingAttributes,
        event_store: Arc<Mutex<EventStore>>,
    ) -> Self {
        Self {
            context: serde_json::to_value(targeting_attributes.clone()).unwrap(),
            event_store,
            targeting_attributes: Some(targeting_attributes.clone()),
        }
    }

    pub(crate) fn update_enrollment(&mut self, enrollment: &ExperimentEnrollment) -> bool {
        if let Some(ref mut targeting_attributes) = self.targeting_attributes {
            targeting_attributes.update_enrollment(enrollment);

            self.context = serde_json::to_value(targeting_attributes.clone()).unwrap();
            true
        } else {
            false
        }
    }
}

pub trait RecordedContext: Send + Sync {
    /// Returns a JSON representation of the context object
    ///
    /// This method will be implemented in foreign code, i.e: Kotlin, Swift, Python, etc...
    fn to_json(&self) -> JsonObject;

    /// Returns a HashMap representation of the event queries that will be used in the targeting
    /// context
    ///
    /// This method will be implemented in foreign code, i.e: Kotlin, Swift, Python, etc...
    fn get_event_queries(&self) -> JsonObject;

    /// Sets the object's internal value for the event query values
    ///
    /// This method will be implemented in foreign code, i.e: Kotlin, Swift, Python, etc...
    fn set_event_query_values(&self, json: JsonObject);

    /// Records the context object to Glean
    ///
    /// This method will be implemented in foreign code, i.e: Kotlin, Swift, Python, etc...
    fn record(&self);
}

impl dyn RecordedContext {
    pub fn execute_queries(
        &self,
        nimbus_targeting_helper: &NimbusTargetingHelper,
    ) -> Result<Map<String, Value>> {
        let results = Map::from_iter(self.get_event_queries().iter().filter_map(
            |(key, value): (&String, &Value)| match value.as_str() {
                Some(v) => match EventQueryType::validate_query(v) {
                    Ok(is_valid) => match is_valid {
                        true => match nimbus_targeting_helper.evaluate_jexl_raw_value(v.into()) {
                            Ok(result) => Some((key.to_string(), result)),
                            Err(err) => {
                                log::info!(
                                    "error during jexl evaluation for query {} — {}",
                                    value,
                                    err.to_string()
                                );
                                None
                            }
                        },
                        false => {
                            log::info!(
                                "key {} with value {} is not a valid event_store query",
                                key,
                                value
                            );
                            Some((key.to_string(), json!(value)))
                        }
                    },
                    Err(err) => {
                        log::error!("{}", err.to_string());
                        None
                    }
                },
                None => {
                    log::info!("value {} for key {} is not a string", key, value);
                    Some((key.to_string(), json!(value)))
                }
            },
        ));
        self.set_event_query_values(results.clone());
        Ok(results)
    }
}
