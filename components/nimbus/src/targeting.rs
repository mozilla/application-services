// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{NimbusError, Result};

use jexl_eval::Evaluator;
use serde::Serialize;
use serde_json::Value;

cfg_if::cfg_if! {
    if #[cfg(feature = "stateful")] {
        use anyhow::anyhow;
        use crate::{TargetingAttributes, stateful::{behavior::{EventStore, EventQueryType, query_event_store}, gecko_prefs::{GeckoPrefStore, query_gecko_pref_store}}};
        use std::sync::{Arc, Mutex};
        use firefox_versioning::compare::version_compare;
    }
}

#[derive(Clone)]
pub struct NimbusTargetingHelper {
    pub(crate) context: Value,
    #[cfg(feature = "stateful")]
    pub(crate) event_store: Arc<Mutex<EventStore>>,
    #[cfg(feature = "stateful")]
    pub(crate) gecko_pref_store: Option<Arc<GeckoPrefStore>>,
    #[cfg(feature = "stateful")]
    pub(crate) targeting_attributes: Option<TargetingAttributes>,
}

impl NimbusTargetingHelper {
    pub fn new<C: Serialize>(
        context: C,
        #[cfg(feature = "stateful")] event_store: Arc<Mutex<EventStore>>,
        #[cfg(feature = "stateful")] gecko_pref_store: Option<Arc<GeckoPrefStore>>,
    ) -> Self {
        Self {
            context: serde_json::to_value(context).unwrap(),
            #[cfg(feature = "stateful")]
            event_store,
            #[cfg(feature = "stateful")]
            gecko_pref_store,
            #[cfg(feature = "stateful")]
            targeting_attributes: None,
        }
    }

    pub fn eval_jexl(&self, expr: String) -> Result<bool> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "stateful")] {
                jexl_eval(&expr, &self.context, self.event_store.clone(), self.gecko_pref_store.clone())
            } else {
                jexl_eval(&expr, &self.context)
            }
        }
    }

    pub fn evaluate_jexl_raw_value(&self, expr: &str) -> Result<Value> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "stateful")] {
                jexl_eval_raw(expr, &self.context, self.event_store.clone(), self.gecko_pref_store.clone())
            } else {
                jexl_eval_raw(expr, &self.context)
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

        Self {
            context,
            #[cfg(feature = "stateful")]
            event_store: self.event_store.clone(),
            #[cfg(feature = "stateful")]
            gecko_pref_store: self.gecko_pref_store.clone(),
            #[cfg(feature = "stateful")]
            targeting_attributes: self.targeting_attributes.clone(),
        }
    }
}

pub fn jexl_eval_raw<Context: serde::Serialize>(
    expression_statement: &str,
    context: &Context,
    #[cfg(feature = "stateful")] event_store: Arc<Mutex<EventStore>>,
    #[cfg(feature = "stateful")] gecko_pref_store: Option<Arc<GeckoPrefStore>>,
) -> Result<Value> {
    let evaluator = Evaluator::new();

    #[cfg(feature = "stateful")]
    let evaluator = evaluator
        .with_transform("versionCompare", |args| Ok(version_compare(args)?))
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
        })
        .with_transform("preferenceIsUserSet", |args| {
            Ok(query_gecko_pref_store(gecko_pref_store.clone(), args)?)
        })
        .with_transform("bucketSample", bucket_sample);

    evaluator
        .eval_in_context(expression_statement, context)
        .map_err(|err| NimbusError::EvaluationError(err.to_string()))
}

// This is the common entry point to JEXL evaluation.
// The targeting attributes and additional context should have been merged and calculated before
// getting here.
// Any additional transforms should be added here.
pub fn jexl_eval<Context: serde::Serialize>(
    expression_statement: &str,
    context: &Context,
    #[cfg(feature = "stateful")] event_store: Arc<Mutex<EventStore>>,
    #[cfg(feature = "stateful")] gecko_pref_store: Option<Arc<GeckoPrefStore>>,
) -> Result<bool> {
    let res = jexl_eval_raw(
        expression_statement,
        context,
        #[cfg(feature = "stateful")]
        event_store,
        #[cfg(feature = "stateful")]
        gecko_pref_store,
    )?;
    match res.as_bool() {
        Some(v) => Ok(v),
        None => Err(NimbusError::InvalidExpression),
    }
}

#[cfg(feature = "stateful")]
fn bucket_sample(args: &[Value]) -> anyhow::Result<Value> {
    fn get_arg_as_u32(args: &[Value], idx: usize, name: &str) -> anyhow::Result<u32> {
        match args.get(idx) {
            None => Err(anyhow!("{} doesn't exist in jexl transform", name)),
            Some(Value::Number(n)) => {
                let n: f64 = if let Some(n) = n.as_u64() {
                    n as f64
                } else if let Some(n) = n.as_i64() {
                    n as f64
                } else if let Some(n) = n.as_f64() {
                    n
                } else {
                    unreachable!();
                };

                debug_assert!(n >= 0.0, "JEXL parser does not support negative values");
                if n > u32::MAX as f64 {
                    Err(anyhow!("{} is out of range", name))
                } else {
                    Ok(n as u32)
                }
            }
            Some(_) => Err(anyhow!("{} is not a number", name)),
        }
    }

    let input = args
        .first()
        .ok_or_else(|| anyhow!("input doesn't exist in jexl transform"))?;
    let start = get_arg_as_u32(args, 1, "start")?;
    let count = get_arg_as_u32(args, 2, "count")?;
    let total = get_arg_as_u32(args, 3, "total")?;

    let result = crate::sampling::bucket_sample(input, start, count, total)?;

    Ok(Value::Bool(result))
}
