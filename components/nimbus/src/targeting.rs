// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{versioning::Version, NimbusError, Result};
use jexl_eval::Evaluator;
use serde::Serialize;
use serde_json::{json, Value};

cfg_if::cfg_if! {
    if #[cfg(feature = "stateful")] {
        use crate::behavior::{EventStore, EventQueryType};
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

// This is the common entry point to JEXL evaluation.
// The targeting attributes and additional context should have been merged and calculated before
// getting here.
// Any additional transforms should be added here.
pub fn jexl_eval<Context: serde::Serialize>(
    expression_statement: &str,
    context: &Context,
    #[cfg(feature = "stateful")] event_store: Arc<Mutex<EventStore>>,
) -> Result<bool> {
    let evaluator =
        Evaluator::new().with_transform("versionCompare", |args| Ok(version_compare(args)?));

    #[cfg(feature = "stateful")]
    let evaluator = evaluator
        .with_transform("eventSum", |args| {
            Ok(query_event_store(
                event_store.clone(),
                EventQueryType::Sum,
                args,
            )?)
        })
        .with_transform("eventCountNonZero", |args| {
            Ok(query_event_store(
                event_store.clone(),
                EventQueryType::CountNonZero,
                args,
            )?)
        })
        .with_transform("eventAveragePerInterval", |args| {
            Ok(query_event_store(
                event_store.clone(),
                EventQueryType::AveragePerInterval,
                args,
            )?)
        })
        .with_transform("eventAveragePerNonZeroInterval", |args| {
            Ok(query_event_store(
                event_store.clone(),
                EventQueryType::AveragePerNonZeroInterval,
                args,
            )?)
        })
        .with_transform("eventLastSeen", |args| {
            Ok(query_event_store(
                event_store.clone(),
                EventQueryType::LastSeen,
                args,
            )?)
        });

    let res = evaluator.eval_in_context(expression_statement, context)?;
    match res.as_bool() {
        Some(v) => Ok(v),
        None => Err(NimbusError::InvalidExpression),
    }
}

fn version_compare(args: &[Value]) -> Result<Value> {
    let curr_version = args.get(0).ok_or_else(|| {
        NimbusError::VersionParsingError("current version doesn't exist in jexl transform".into())
    })?;
    let curr_version = curr_version.as_str().ok_or_else(|| {
        NimbusError::VersionParsingError("current version in jexl transform is not a string".into())
    })?;
    let min_version = args.get(1).ok_or_else(|| {
        NimbusError::VersionParsingError("minimum version doesn't exist in jexl transform".into())
    })?;
    let min_version = min_version.as_str().ok_or_else(|| {
        NimbusError::VersionParsingError("minium version is not a string in jexl transform".into())
    })?;
    let min_version = Version::try_from(min_version)?;
    let curr_version = Version::try_from(curr_version)?;
    Ok(json!(if curr_version > min_version {
        1
    } else if curr_version < min_version {
        -1
    } else {
        0
    }))
}

#[cfg(feature = "stateful")]
fn query_event_store(
    event_store: Arc<Mutex<EventStore>>,
    query_type: EventQueryType,
    args: &[Value],
) -> Result<Value> {
    let (event, interval, num_buckets, starting_bucket) = query_type.validate_arguments(args)?;

    Ok(json!(event_store.lock().unwrap().query(
        &event,
        interval,
        num_buckets,
        starting_bucket,
        query_type,
    )?))
}
