// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::stateful::gecko_prefs::GeckoPrefStore;
use crate::{
    enrollment::ExperimentEnrollment,
    error::{warn, BehaviorError},
    json::JsonObject,
    stateful::behavior::{EventQueryType, EventStore},
    NimbusError, NimbusTargetingHelper, Result, TargetingAttributes,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

impl NimbusTargetingHelper {
    pub(crate) fn with_targeting_attributes(
        targeting_attributes: &TargetingAttributes,
        event_store: Arc<Mutex<EventStore>>,
        pref_store: Option<Arc<GeckoPrefStore>>,
    ) -> Self {
        Self {
            context: serde_json::to_value(targeting_attributes.clone()).unwrap(),
            event_store,
            gecko_pref_store: pref_store,
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
    fn get_event_queries(&self) -> HashMap<String, String>;

    /// Sets the object's internal value for the event query values
    ///
    /// This method will be implemented in foreign code, i.e: Kotlin, Swift, Python, etc...
    fn set_event_query_values(&self, event_query_values: HashMap<String, f64>);

    /// Records the context object to Glean
    ///
    /// This method will be implemented in foreign code, i.e: Kotlin, Swift, Python, etc...
    fn record(&self);
}

impl dyn RecordedContext {
    pub fn execute_queries(
        &self,
        nimbus_targeting_helper: &NimbusTargetingHelper,
    ) -> Result<HashMap<String, f64>> {
        let results: HashMap<String, f64> =
            HashMap::from_iter(self.get_event_queries().iter().filter_map(|(key, query)| {
                match nimbus_targeting_helper.evaluate_jexl_raw_value(query) {
                    Ok(result) => match result.as_f64() {
                        Some(v) => Some((key.clone(), v)),
                        None => {
                            warn!(
                                "Value '{}' for query '{}' was not a string",
                                result.to_string(),
                                query
                            );
                            None
                        }
                    },
                    Err(err) => {
                        let error_string = format!(
                            "error during jexl evaluation for query '{}' — {}",
                            query, err
                        );
                        warn!("{}", error_string);
                        None
                    }
                }
            }));
        self.set_event_query_values(results.clone());
        Ok(results)
    }

    pub fn validate_queries(&self) -> Result<()> {
        for query in self.get_event_queries().values() {
            match EventQueryType::validate_query(query) {
                Ok(true) => continue,
                Ok(false) => {
                    return Err(NimbusError::BehaviorError(
                        BehaviorError::EventQueryParseError(query.clone()),
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
