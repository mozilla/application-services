/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, LazyLock};
use tracing::{subscriber::Interest, Metadata};
use tracing_subscriber::{
    layer::{Context, Filter},
    Layer,
};

use crate::EventSink;
use tracing::field::{Field, Visit};

struct LogEntry {
    level: tracing::Level,
    sink: Arc<dyn EventSink>,
}

static SINKS_BY_TARGET: LazyLock<RwLock<HashMap<String, LogEntry>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

static MIN_LEVEL_SINK: RwLock<Option<LogEntry>> = RwLock::new(None);

pub fn register_event_sink(target: &str, level: crate::Level, sink: Arc<dyn EventSink>) {
    SINKS_BY_TARGET.write().insert(
        target.to_string(),
        LogEntry {
            level: level.into(),
            sink,
        },
    );
}

/// Register an event sink that will receive events based on a minimum level
///
/// If an event's level is at least `level`, then the event will be sent to this sink.
/// If so, sinks registered with `register_event_sink` will be skiped.
///
/// There can only be 1 min-level sink registered at once.
pub fn register_min_level_event_sink(level: crate::Level, sink: Arc<dyn EventSink>) {
    *MIN_LEVEL_SINK.write() = Some(LogEntry {
        level: level.into(),
        sink,
    });
}

#[uniffi::export]
pub fn unregister_event_sink(target: &str) {
    SINKS_BY_TARGET.write().remove(target);
}

/// Remove the sink registered with [register_min_level_event_sink], if any.
#[uniffi::export]
pub fn unregister_min_level_event_sink() {
    *MIN_LEVEL_SINK.write() = None;
}

// UniFFI versions of the registration functions.  This input a Box to be compatible with callback
// interfaces
#[uniffi::export(name = "register_event_sink")]
pub fn register_event_sink_box(target: &str, level: crate::Level, sink: Box<dyn EventSink>) {
    register_event_sink(target, level, sink.into())
}

#[uniffi::export(name = "register_min_level_event_sink")]
pub fn register_min_level_event_sink_box(level: crate::Level, sink: Box<dyn EventSink>) {
    register_min_level_event_sink(level, sink.into())
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
        let target = event.metadata().target();
        let prefix = match target.find(':') {
            Some(index) => &target[..index],
            None => target,
        };
        if let Some(entry) = &*MIN_LEVEL_SINK.read() {
            if entry.level >= *event.metadata().level() {
                entry.send_event(event);
                return;
            }
        }

        if let Some(entry) = SINKS_BY_TARGET.read().get(prefix) {
            let level = *event.metadata().level();
            if level <= entry.level {
                entry.send_event(event);
            }
        }
    }
}

impl LogEntry {
    fn send_event(&self, event: &tracing::Event<'_>) {
        let mut fields = BTreeMap::new();
        let mut message = String::default();
        let mut visitor = JsonVisitor(&mut message, &mut fields);
        event.record(&mut visitor);
        let event = crate::Event {
            level: (*event.metadata().level()).into(),
            target: event.metadata().target().to_string(),
            name: event.metadata().name().to_string(),
            message,
            fields: serde_json::to_value(&fields).unwrap_or_default(),
        };
        self.sink.on_event(event);
    }
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
