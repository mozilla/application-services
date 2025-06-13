/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::{Arc, Once, OnceLock};

static MAX_LEVEL: OnceLock<Level> = OnceLock::new();
static FOREIGN_LOGGER: OnceLock<Box<dyn AppServicesLogger>> = OnceLock::new();
static GLOBAL_SUBSCRIBER: Once = Once::new();

// The "targets" (in practice, crate names) which are hooked up to the `tracing` crate for logging.
// We should improve this, or better still, just kill this crate entirely and move to using
// tracing-support directly, like we (plan to) do on Desktop.
//
// Note also that it is a natural consequence of using `tracing` that each target must be explicitly listened for
// *somewhere*, otherwise logs from that target will not be seen. For now, that list of targets is here, but
// when we move to tracing-support it's likely this list will be pushed down into the clients (so that each
// target can optionally be handled differently from the others).
static TRACING_TARGETS: &[&str] = &[
    "autofill",
    "error_support",
    "fxa-client",
    "init_rust_components",
    "interrupt_support",
    "logins",
    "merino",
    "nimbus",
    "places",
    "push",
    "rate-limiter",
    "rc_crypto",
    "relevancy",
    "remote_settings",
    "search",
    "sql_support",
    "suggest",
    "sync_manager",
    "sync15",
    "tabs",
    "viaduct",
];

#[derive(uniffi::Record, Debug, PartialEq, Eq)]
pub struct Record {
    pub level: Level,
    pub target: String,
    pub message: String,
}

// ideally we'd use tracing::Level as an external type, but that would cause a breaking change
// for mobile, so we clone it.
// (it's a shame uniffi can't re-export types!)
#[derive(uniffi::Enum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<tracing_support::Level> for Level {
    fn from(level: tracing_support::Level) -> Self {
        match level {
            tracing_support::Level::Error => Level::Error,
            tracing_support::Level::Warn => Level::Warn,
            tracing_support::Level::Info => Level::Info,
            tracing_support::Level::Debug => Level::Debug,
            tracing_support::Level::Trace => Level::Trace,
        }
    }
}

impl From<Level> for tracing_support::Level {
    fn from(level: Level) -> Self {
        match level {
            Level::Error => tracing_support::Level::Error,
            Level::Warn => tracing_support::Level::Warn,
            Level::Info => tracing_support::Level::Info,
            Level::Debug => tracing_support::Level::Debug,
            Level::Trace => tracing_support::Level::Trace,
        }
    }
}

#[uniffi::export(callback_interface)]
pub trait AppServicesLogger: Sync + Send {
    fn log(&self, record: Record);
}

/// Set the logger to forward to.
///
/// Pass in None to disable logging.
#[uniffi::export]
pub fn set_logger(logger: Option<Box<dyn AppServicesLogger>>) {
    GLOBAL_SUBSCRIBER.call_once(|| {
        use tracing_subscriber::prelude::*;
        tracing_subscriber::registry()
            .with(tracing_support::simple_event_layer())
            .init();
    });

    let level = MAX_LEVEL.get_or_init(|| Level::Debug);
    let sink = Arc::new(ForwarderEventSink {});
    // Set up a tracing subscriber for crates which use tracing and forward to the foreign log forwarder.
    for target in TRACING_TARGETS {
        tracing_support::register_event_sink(target, (*level).into(), sink.clone())
    }
    // if called before we just ignore the error for now, and also ignored if they supply None.
    if let Some(logger) = logger {
        FOREIGN_LOGGER.set(logger).ok();
    }
}

/// Set the maximum log level filter.  Records below this level will not be sent to the logger.
/// You must set this exactly once, before you call `set_logger()`
#[uniffi::export]
pub fn set_max_level(level: Level) {
    MAX_LEVEL.set(level).ok();
}

struct ForwarderEventSink;

impl tracing_support::EventSink for ForwarderEventSink {
    fn on_event(&self, event: tracing_support::Event) {
        let record = Record {
            level: event.level.into(),
            target: event.target,
            message: event.message,
        };
        if let Some(foreign_logger) = FOREIGN_LOGGER.get() {
            foreign_logger.log(record);
        }
    }
}

uniffi::setup_scaffolding!("rust_log_forwarder");

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct TestLogger {
        records: Arc<Mutex<Vec<Record>>>,
    }

    impl TestLogger {
        fn new() -> Self {
            Self {
                records: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn check_records(&self, correct_records: Vec<Record>) {
            assert_eq!(*self.records.lock().unwrap(), correct_records);
        }

        fn clear_records(&self) {
            self.records.lock().unwrap().clear()
        }
    }

    impl AppServicesLogger for TestLogger {
        fn log(&self, record: Record) {
            self.records.lock().unwrap().push(record)
        }
    }

    // Lock that we take for each test.  This prevents multiple threads from running these tests at
    // the same time, which makes them flakey.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_logging() {
        let _lock = TEST_LOCK.lock().unwrap();
        let logger = TestLogger::new();
        set_max_level(Level::Debug);
        set_logger(Some(Box::new(logger.clone())));
        // new tracing subscriber for our test.
        let sink = Arc::new(ForwarderEventSink {});
        tracing_support::register_event_sink("rust_log_forwarder", Level::Debug.into(), sink);

        tracing_support::info!("Test message");
        tracing_support::warn!("Test message2");
        logger.check_records(vec![
            Record {
                level: Level::Info,
                target: "rust_log_forwarder::test".into(),
                message: "Test message".into(),
            },
            Record {
                level: Level::Warn,
                target: "rust_log_forwarder::test".into(),
                message: "Test message2".into(),
            },
        ]);
        logger.clear_records();
        set_logger(None);
        //log::info!("Test message");
        //log::warn!("Test message2");
        logger.check_records(vec![]);
    }
}
