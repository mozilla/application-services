use crate::{
    enrollment::ExperimentEnrollment, stateful::behavior::EventStore, NimbusTargetingHelper,
    TargetingAttributes,
};
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
