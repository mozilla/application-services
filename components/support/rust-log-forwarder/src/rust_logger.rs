/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Rust Logger implementation
//!
//! This is responsible for taking logs from the rust log crate and forwarding them to a
//! foreign_logger::Logger instance.

use crate::foreign_logger::Logger as ForeignLogger;
use parking_lot::RwLock;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Once,
};

// ForeignLogger to forward to
static RUST_LOGGER: Logger = Logger::new();
// Handles calling `log::set_logger`, which can only be called once.
static INIT: Once = Once::new();

struct Logger {
    foreign_logger: RwLock<Option<Box<dyn ForeignLogger>>>,
    is_enabled: AtomicBool,
}

impl Logger {
    const fn new() -> Self {
        Self {
            foreign_logger: RwLock::new(None),
            is_enabled: AtomicBool::new(false),
        }
    }

    fn set_foreign_logger(&self, foreign_logger: Option<Box<dyn ForeignLogger>>) {
        self.is_enabled
            .store(foreign_logger.is_some(), Ordering::Relaxed);
        *self.foreign_logger.write() = foreign_logger;
    }
}

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        self.is_enabled.load(Ordering::Relaxed)
    }

    fn log(&self, record: &log::Record<'_>) {
        if let Some(foreign_logger) = &*self.foreign_logger.read() {
            foreign_logger.log(record.into())
        }
    }

    fn flush(&self) {}
}

pub fn set_foreign_logger(foreign_logger: Option<Box<dyn ForeignLogger>>) {
    INIT.call_once(|| {
        // This should be the only component that calls `log::set_logger()`.  If not, then
        // panic'ing seems reasonable.
        log::set_logger(&RUST_LOGGER).expect(
            "Failed to initialize rust-log-forwarder::Logger, other log implementation already initialized?",
        );
    });
    RUST_LOGGER.set_foreign_logger(foreign_logger);
}
