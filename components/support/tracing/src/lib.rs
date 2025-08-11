/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod layer;

#[cfg(feature = "testing")]
mod testing;

#[cfg(feature = "testing")]
pub use testing::{init_for_tests, init_for_tests_with_level};

pub use layer::{
    register_event_sink, register_min_level_event_sink, simple_event_layer, unregister_event_sink,
    unregister_min_level_event_sink,
};
// Re-export tracing so that our dependencies can use it.
pub use tracing;

// Define standard logging macros.
//
// These all add `tracing_support = true`, which we use as an event filter in our layer.
// This will statically disable the layer for events from outside crates that use tracing.
// This way we don't pay a performance penalty for those events.
// See `SimpleEventFilter` for details.

#[macro_export]
macro_rules! trace {
    (target: $target:expr, $($tt:tt)*) => {
        $crate::tracing::trace!(
        target: $target,
        tracing_support = true,
        $($tt)*)
    };
    ($($tt:tt)*) => {
        $crate::tracing::trace!(
        tracing_support = true,
        $($tt)*)
    };
}

#[macro_export]
macro_rules! debug {
    (target: $target:expr, $($tt:tt)*) => {
        $crate::tracing::debug!(
        target: $target,
        tracing_support = true,
        $($tt)*)
    };
    ($($tt:tt)*) => {
        $crate::tracing::debug!(
        tracing_support = true,
        $($tt)*)
    };
}

#[macro_export]
macro_rules! info {
    (target: $target:expr, $($tt:tt)*) => {
        $crate::tracing::info!(
        target: $target,
        tracing_support = true,
        $($tt)*)
    };
    ($($tt:tt)*) => {
        $crate::tracing::info!(
        tracing_support = true,
        $($tt)*)
    };
}

#[macro_export]
macro_rules! warn {
    (target: $target:expr, $($tt:tt)*) => {
        $crate::tracing::warn!(
        target: $target,
        tracing_support = true,
        $($tt)*)
    };
    ($($tt:tt)*) => {
        $crate::tracing::warn!(
        tracing_support = true,
        $($tt)*)
    };
}

#[macro_export]
macro_rules! error {
    (target: $target:expr, $($tt:tt)*) => {
        $crate::tracing::error!(
        target: $target,
        tracing_support = true,
        $($tt)*)
    };
    ($($tt:tt)*) => {
        $crate::tracing::error!(
        tracing_support = true,
        $($tt)*)
    };
}

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
            .with(layer::simple_event_layer())
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
        let level_sink = Arc::new(Sink::new());

        crate::layer::register_event_sink("first_target", Level::Info, sink.clone());
        crate::layer::register_event_sink("second_target", Level::Debug, sink.clone());

        // Only 1 sink can be registered with `register_min_level_event_sink`.  The first call
        // should be ignored and only the second call should take effect.
        crate::layer::register_min_level_event_sink(Level::Warn, sink.clone());
        crate::layer::register_min_level_event_sink(Level::Error, level_sink.clone());

        info!(target: "first_target", extra=-1, "event message");
        debug!(target: "first_target", extra=-2, "event message (should be filtered)");
        debug!(target: "second_target", extra=-3, "event message2");
        info!(target: "third_target", extra=-4, "event message (should be filtered)");
        // This should only go to the level sink, since it's an error
        error!(target: "first_target", extra=-5, "event message");

        assert_eq!(sink.events.read().len(), 2);
        assert_eq!(level_sink.events.read().len(), 1);

        let event = &sink.events.read()[0];
        assert_eq!(event.target, "first_target");
        assert_eq!(event.level, Level::Info);
        assert_eq!(event.message, "event message");
        assert_eq!(event.fields.get("extra").unwrap().as_i64(), Some(-1));

        let event2 = &sink.events.read()[1];
        assert_eq!(event2.target, "second_target");
        assert_eq!(event2.level, Level::Debug);
        assert_eq!(event2.message, "event message2");
        assert_eq!(event2.fields.get("extra").unwrap().as_i64(), Some(-3));

        let event3 = &level_sink.events.read()[0];
        assert_eq!(event3.target, "first_target");
        assert_eq!(event3.level, Level::Error);
        assert_eq!(event3.message, "event message");
        assert_eq!(event3.fields.get("extra").unwrap().as_i64(), Some(-5));
    }
}
