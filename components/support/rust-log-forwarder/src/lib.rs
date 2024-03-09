/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
mod foreign_logger;
mod rust_logger;

pub use foreign_logger::{AppServicesLogger, Level, Record};

static HAVE_SET_MAX_LEVEL: AtomicBool = AtomicBool::new(false);

// The "targets" (in practice, crate names) which are hooked up to the `tracing` crate for logging.
// We should improve this, or better still, just kill this crate entirely
static TRACING_TARGETS: &[&str] = &["autofill", "tabs"];

/// Set the logger to forward to.
///
/// Pass in None to disable logging.
pub fn set_logger(logger: Option<Box<dyn AppServicesLogger>>) {
    // Set a default max level, if none has already been set
    if !HAVE_SET_MAX_LEVEL.load(Ordering::Relaxed) {
        set_max_level(Level::Debug);
    }

    let sink = Arc::new(ForwarderEventSink {});
    // Set up a tracing subscriber for crates which use tracing and forward to the log forwarder.
    for target in TRACING_TARGETS {
        tracing_support::register_event_sink_arc(
            target,
            tracing_support::Level::Trace,
            sink.clone(),
        )
    }
    rust_logger::set_foreign_logger(logger)
}

struct ForwarderEventSink;

impl tracing_support::EventSink for ForwarderEventSink {
    fn on_event(&self, event: tracing_support::Event) {
        let record = Record {
            level: match event.level {
                tracing_support::Level::Trace => Level::Trace,
                tracing_support::Level::Debug => Level::Debug,
                tracing_support::Level::Info => Level::Info,
                tracing_support::Level::Warn => Level::Warn,
                tracing_support::Level::Error => Level::Error,
            },
            target: event.target,
            message: event.message,
        };
        rust_logger::forward_to_foreign_logger(record);
    }
}

/// Set the maximum log level filter.  Records below this level will not be sent to the logger.
pub fn set_max_level(level: Level) {
    log::set_max_level(level.to_level_filter());
    HAVE_SET_MAX_LEVEL.store(true, Ordering::Relaxed);
}

uniffi::include_scaffolding!("rust_log_forwarder");

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
        set_logger(Some(Box::new(logger.clone())));
        set_max_level(Level::Debug);
        log::info!("Test message");
        log::warn!("Test message2");
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
        log::info!("Test message");
        log::warn!("Test message2");
        logger.check_records(vec![]);
    }

    #[test]
    fn test_max_level() {
        let _lock = TEST_LOCK.lock().unwrap();
        set_max_level(Level::Debug);
        assert_eq!(log::max_level(), log::Level::Debug);
        set_max_level(Level::Warn);
        assert_eq!(log::max_level(), log::Level::Warn);
    }

    #[test]
    fn test_max_level_default() {
        let _lock = TEST_LOCK.lock().unwrap();
        HAVE_SET_MAX_LEVEL.store(false, Ordering::Relaxed);
        let logger = TestLogger::new();
        // Calling set_logger should set the level to `Debug' by default
        set_logger(Some(Box::new(logger)));
        assert_eq!(log::max_level(), log::Level::Debug);
    }

    #[test]
    fn test_max_level_default_ignored_if_set_manually() {
        let _lock = TEST_LOCK.lock().unwrap();
        HAVE_SET_MAX_LEVEL.store(false, Ordering::Relaxed);
        set_max_level(Level::Warn);
        // Calling set_logger should not set the level if it was set manually.
        set_logger(Some(Box::new(TestLogger::new())));
        assert_eq!(log::max_level(), log::Level::Warn);
    }
}
