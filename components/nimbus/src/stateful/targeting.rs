// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    enrollment::ExperimentEnrollment,
    error::BehaviorError,
    json::JsonObject,
    stateful::behavior::{EventQueryType, EventStore},
    NimbusError, NimbusTargetingHelper, Result, TargetingAttributes,
};
use serde_json::Map;
use serde_json::Value;
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
        let results = self
            .get_event_queries()
            .iter()
            .filter_map(|(key, value)| {
                match value
                    .as_str()
                    .map(|v| nimbus_targeting_helper.evaluate_jexl_raw_value(v))
                {
                    Some(Ok(result)) => Some((key.clone(), result)),
                    Some(Err(err)) => {
                        log::info!("error during jexl evaluation for query {} — {}", value, err);
                        None
                    }
                    None => {
                        log::info!("value {} for key {} is not a string", key, value);
                        Some((key.to_string(), serde_json::json!(value)))
                    }
                }
            })
            .collect::<Map<_, _>>();
        self.set_event_query_values(results.clone());
        Ok(results)
    }

    pub fn validate_queries(&self) -> Result<()> {
        for value in self.get_event_queries().values() {
            let query = value.as_str().ok_or_else(|| {
                NimbusError::BehaviorError(BehaviorError::TypeError(
                    value.to_string(),
                    "string".into(),
                ))
            })?;

            match EventQueryType::validate_query(query) {
                Ok(true) => continue,
                Ok(false) => {
                    return Err(NimbusError::BehaviorError(
                        BehaviorError::EventQueryParseError(query.into()),
                    ));
                }
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }
}

pub fn validate_event_queries(recorded_context: Arc<dyn RecordedContext>) -> Result<()> {
    recorded_context.validate_queries()
}
