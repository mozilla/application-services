// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{evaluator::jexl_eval, Result};
use serde::Serialize;
use serde_json::Value;

cfg_if::cfg_if! {
    if #[cfg(feature = "stateful")] {
        use crate::behavior::EventStore;
        use std::sync::{Arc, Mutex};
    }
}

pub struct NimbusTargetingHelper {
    pub(crate) context: Value,
    #[cfg(feature = "stateful")]
    pub(crate) event_store: Arc<Mutex<EventStore>>,
}

impl NimbusTargetingHelper {
    pub fn new<C: Serialize>(
        context: C,
        #[cfg(feature = "stateful")] event_store: Arc<Mutex<EventStore>>,
    ) -> Self {
        Self {
            context: serde_json::to_value(context).unwrap(),
            #[cfg(feature = "stateful")]
            event_store,
        }
    }

    pub fn eval_jexl(&self, expr: String) -> Result<bool> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "stateful")] {
                jexl_eval(&expr, &self.context, self.event_store.clone())
            } else {
                jexl_eval(&expr, &self.context)
            }
        }
    }

    pub(crate) fn put(&self, key: &str, value: bool) -> Self {
        let context = if let Value::Object(map) = &self.context {
            let mut map = map.clone();
            map.insert(key.to_string(), Value::Bool(value));
            Value::Object(map)
        } else {
            self.context.clone()
        };

        #[cfg(feature = "stateful")]
        let event_store = self.event_store.clone();
        Self {
            context,
            #[cfg(feature = "stateful")]
            event_store,
        }
    }
}
