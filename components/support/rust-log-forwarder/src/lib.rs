/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::atomic::{AtomicBool, Ordering};
mod foreign_logger;
mod rust_logger;

pub use foreign_logger::{Level, Logger, Record};

static HAVE_SET_MAX_LEVEL: AtomicBool = AtomicBool::new(false);

/// Set the logger to forward to.
///
/// Pass in None to disable logging.
pub fn set_logger(logger: Option<Box<dyn Logger>>) {
    // Set a default max level, if none has already been set
    if !HAVE_SET_MAX_LEVEL.load(Ordering::Relaxed) {
        set_max_level(Level::Debug);
    }
    rust_logger::set_foreign_logger(logger)
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

    impl Logger for TestLogger {
        fn log(&self, record: Record) {
            self.records.lock().unwrap().push(record)
        }
    }

    #[test]
    fn test_logging() {
        let logger = TestLogger::new();
        set_logger(Some(Box::new(logger.clone())));
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
        set_max_level(Level::Debug);
        assert_eq!(log::max_level(), log::Level::Debug);
        set_max_level(Level::Warn);
        assert_eq!(log::max_level(), log::Level::Warn);
    }

    #[test]
    fn test_max_level_default() {
        HAVE_SET_MAX_LEVEL.store(false, Ordering::Relaxed);
        let logger = TestLogger::new();
        // Calling set_logger should set the level to `Debug' by default
        set_logger(Some(Box::new(logger)));
        assert_eq!(log::max_level(), log::Level::Debug);
    }

    #[test]
    fn test_max_level_default_ignored_if_set_manually() {
        HAVE_SET_MAX_LEVEL.store(false, Ordering::Relaxed);
        set_max_level(Level::Warn);
        // Calling set_logger should not set the level if it was set manually.
        set_logger(Some(Box::new(TestLogger::new())));
        assert_eq!(log::max_level(), log::Level::Warn);
    }
}
