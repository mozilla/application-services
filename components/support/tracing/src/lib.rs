mod layer;

#[cfg(feature = "testing")]
mod testing;

#[cfg(feature = "testing")]
pub use testing::{init_for_tests, init_for_tests_with_level};

pub use layer::{register_event_sink, unregister_event_sink, SimpleEventLayer};
pub use tracing::{debug, error, info, trace, warn};

// grr - swift has name collision with `Level`? Can uniifi help make this cleaner?
pub type Level = TracingLevel;

// `tracing::Level` is a struct, we want an enum for both uniffi and `log::Level`` compat.
#[derive(uniffi::Enum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum TracingLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<tracing::Level> for Level {
    fn from(level: tracing::Level) -> Level {
        if level == tracing::Level::ERROR {
            Level::Error
        } else if level == tracing::Level::WARN {
            Level::Warn
        } else if level == tracing::Level::INFO {
            Level::Info
        } else if level == tracing::Level::DEBUG {
            Level::Debug
        } else if level == tracing::Level::TRACE {
            Level::Trace
        } else {
            unreachable!();
        }
    }
}

impl From<Level> for tracing::Level {
    fn from(level: Level) -> Self {
        match level {
            Level::Error => tracing::Level::ERROR,
            Level::Warn => tracing::Level::WARN,
            Level::Info => tracing::Level::INFO,
            Level::Debug => tracing::Level::DEBUG,
            Level::Trace => tracing::Level::TRACE,
        }
    }
}

// ditto grr re swift name collisions.
pub type Event = TracingEvent;

#[derive(uniffi::Record, Debug)]
pub struct TracingEvent {
    pub level: Level,
    pub target: String,
    pub name: String,
    pub message: String,
    pub fields: serde_json::Value,
}

// uniffi foreign trait.
// #[uniffi::export(with_foreign)]
// oh no - for now, a callback :(
#[uniffi::export(callback_interface)]
pub trait EventSink: Send + Sync {
    fn on_event(&self, event: Event);
}

use serde_json::Value as TracingJsonValue;

uniffi::custom_type!(TracingJsonValue, String, {
    remote,
    // Lowering serde_json::Value into a String.
    lower: |s| s.to_string(),
    // Lifting our foreign String into a serde_json::Value
    try_lift: |s| {
        Ok(serde_json::from_str(s.as_str()).unwrap())
    },
});

uniffi::setup_scaffolding!("tracing");

#[cfg(test)]
mod tests {
    use parking_lot::RwLock;
    use std::sync::Arc;

    use super::*;

    #[test]
    fn test_app() {
        use tracing_subscriber::prelude::*;
        tracing_subscriber::registry()
            .with(layer::SimpleEventLayer)
            .init();

        struct Sink {
            events: RwLock<Vec<Event>>,
        }

        impl Sink {
            fn new() -> Self {
                Self {
                    events: RwLock::new(Vec::new()),
                }
            }
        }

        impl EventSink for Sink {
            fn on_event(&self, event: Event) {
                self.events.write().push(event);
            }
        }
        let sink = Arc::new(Sink::new());

        crate::layer::register_event_sink("first_target", Level::Info, sink.clone());
        crate::layer::register_event_sink("second_target", Level::Info, sink.clone());

        tracing::event!(target: "first_target", tracing::Level::INFO, extra = -1, "event message");

        assert_eq!(sink.events.read().len(), 1);
        let event = &sink.events.read()[0];
        assert_eq!(event.target, "first_target");
        assert_eq!(event.level, Level::Info);
        assert_eq!(event.message, "event message");
        assert_eq!(event.fields.get("extra").unwrap().as_i64(), Some(-1));
    }
}
