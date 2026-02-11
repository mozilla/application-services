/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use parking_lot::{const_rwlock, RwLock};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use tracing::{subscriber::Interest, Metadata};
use tracing_subscriber::{
    layer::{Context, Filter},
    Layer,
};

use crate::{EventSink, Level};
use tracing::field::{Field, Visit};

static SINKS: RwLock<Vec<RegisteredEventSink>> = const_rwlock(Vec::new());
static EVENT_SINK_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EventSinkId(u32);

uniffi::custom_type!(EventSinkId, u32, {
    try_lift: |raw_id| Ok(EventSinkId(raw_id)),
    lower: |sink_id| sink_id.0,
});

/// Register an event sink using an [EventSinkSpecification]
///
/// Returns an [EventSinkId] that can be used to unregister the sink.
pub fn register_event_sink(spec: EventSinkSpecification, sink: Arc<dyn EventSink>) -> EventSinkId {
    let id = EventSinkId(EVENT_SINK_COUNTER.fetch_add(1, Ordering::Relaxed));
    SINKS.write().push(RegisteredEventSink { id, spec, sink });
    id
}

struct RegisteredEventSink {
    // ID that can be used to unregister this sink
    id: EventSinkId,
    spec: EventSinkSpecification,
    sink: Arc<dyn EventSink>,
}

#[derive(uniffi::Record)]
/// Describes which events to an EventSink
pub struct EventSinkSpecification {
    // Send events that match these targets/levels
    #[uniffi(default)]
    pub targets: Vec<EventTarget>,
    // Send events have a `min_level` or above.
    #[uniffi(default)]
    pub min_level: Option<Level>,
}

#[derive(uniffi::Record, Debug)]
pub struct EventTarget {
    pub target: String,
    pub level: Level,
}

#[uniffi::export]
pub fn unregister_event_sink(id: EventSinkId) {
    SINKS.write().retain(|info| info.id != id);
}

// UniFFI versions of the registration functions.  This input a Box to be compatible with callback
// interfaces

#[uniffi::export(name = "register_event_sink")]
pub fn register_event_sink_box(
    targets: EventSinkSpecification,
    sink: Box<dyn EventSink>,
) -> EventSinkId {
    register_event_sink(targets, sink.into())
}

pub fn simple_event_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    SimpleEventLayer.with_filter(SimpleEventFilter)
}

pub struct SimpleEventLayer;

impl<S> Layer<S> for SimpleEventLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let sinks = find_sinks_for_event(event);
        if sinks.is_empty() {
            // return early to skip the conversion below
            return;
        }
        let mut fields = BTreeMap::new();
        let mut message = String::default();
        let mut visitor = JsonVisitor(&mut message, &mut fields);
        event.record(&mut visitor);
        let tracing_event = crate::Event {
            level: (*event.metadata().level()).into(),
            target: event.metadata().target().to_string(),
            name: event.metadata().name().to_string(),
            message,
            fields: serde_json::to_value(&fields).unwrap_or_default(),
        };
        for sink in sinks {
            sink.on_event(tracing_event.clone());
        }
    }
}

/// Find event sinks that match an event.
fn find_sinks_for_event(event: &tracing::Event<'_>) -> Vec<Arc<dyn EventSink>> {
    let target = event.metadata().target();
    let prefix = match target.find(':') {
        Some(index) => &target[..index],
        None => target,
    };
    let level = Level::from(*event.metadata().level());

    // This requires a iterating through the entire SINKS vec, which could have performance impacts
    // if we have many sinks registered.  In practice, there should only be a handful of these so
    // this should be fine.
    SINKS
        .read()
        .iter()
        .filter_map(|info| {
            if let Some(min_level) = &info.spec.min_level {
                if *min_level >= level {
                    return Some(info.sink.clone());
                }
            }
            for target in info.spec.targets.iter() {
                if target.target == prefix && target.level >= level {
                    return Some(info.sink.clone());
                }
            }
            None
        })
        .collect()
}

struct SimpleEventFilter;

impl SimpleEventFilter {
    /// Check if we should process events from a callsite
    fn should_process_callsite(&self, meta: &Metadata<'_>) -> bool {
        if meta.fields().field("tracing_support").is_some() {
            // Event came from `tracing_support`'s logging macros.
            // Enable the layer for this callsite.
            // Whether we actually do anything for an event is controlled by `SimpleEventLayer.on_event()`
            true
        } else {
            // Event came from a crate not using `tracing_support`, we don't want to handle it.
            // By returning `Interest::never`, we avoid the lock + map lookup.
            false
        }
    }
}

impl<S> Filter<S> for SimpleEventFilter
where
    S: tracing::Subscriber,
{
    fn callsite_enabled(&self, meta: &Metadata<'_>) -> Interest {
        if self.should_process_callsite(meta) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn enabled(&self, meta: &Metadata<'_>, _cx: &Context<'_, S>) -> bool {
        self.should_process_callsite(meta)
    }
}

// from https://burgers.io/custom-logging-in-rust-using-tracing
struct JsonVisitor<'a>(&'a mut String, &'a mut BTreeMap<String, serde_json::Value>);

impl JsonVisitor<'_> {
    fn record_str_value(&mut self, field_name: &str, value: String) {
        if field_name == "message" {
            *self.0 = value.to_string()
        } else {
            self.1
                .insert(field_name.to_string(), serde_json::json!(value));
        }
    }
}

impl Visit for JsonVisitor<'_> {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.1
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.1
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.1
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.1
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_str_value(field.name(), value.to_string());
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.record_str_value(field.name(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_str_value(field.name(), format!("{:?}", value));
    }
}
