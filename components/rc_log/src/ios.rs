/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::LogLevel;
use std::os::raw::c_char;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Type of the log callback provided to us by swift.
/// Takes the following arguments:
///
/// - Log level (an i32).
/// - Tag: a (nullable) utf-8 encoded string. Caller must not free this string!
/// - TagLen: length of the tag string in bytes, or 0 if it was null.
/// - Message: a (non-nullable) nul terminated c string. Caller must not free this string!
/// - MessageLen: Length of the meessage string in bytes.
///
/// and returns 0 if we should close the thread, and 1 otherwise. This is done because
/// attempting to call `close` from within the log callback will deadlock.
pub type LogCallback = unsafe extern "C" fn(i32, *const c_char, usize, *const c_char, usize) -> u8;

pub struct LogAdapterState {
    stop: Arc<AtomicBool>,
}

struct Logger {
    callback: LogCallback,
    stop: Arc<AtomicBool>,
}

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        !self.stop.load(Ordering::SeqCst)
    }

    fn flush(&self) {}
    fn log(&self, record: &log::Record) {
        if !self.stop.load(Ordering::SeqCst) {
            // Note: `enabled` is not automatically called.
            return;
        }
        let (tag_ptr, tag_len) = record
            .module_path()
            .map(|s| (s.as_ptr() as *const c_char, s.len()))
            .unwrap_or_else(|| (std::ptr::null(), 0));

        // TODO: use SmallVec<[u8; 4096]> or something?
        let msg_str = format!("{}", record.args());

        let msg_ptr = msg_str.as_ptr() as *const c_char;
        let msg_len = msg_str.len();

        let level: LogLevel = record.level().into();

        let stop = unsafe { (self.callback)(level as i32, tag_ptr, tag_len, msg_ptr, msg_len) };
        if stop != 0 {
            // Set ourselves as disabled.
            self.stop.store(true, Ordering::SeqCst);
        }
    }
}

impl LogAdapterState {
    pub fn init(callback: LogCallback) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let log = Logger {
            callback,
            stop: stop.clone(),
        };
        crate::settable_log::set_logger(Box::new(log));
        log::set_max_level(log::LevelFilter::Debug);
        log::info!("rc_log adapter initialized!");
        Self { stop }
    }
}

impl Drop for LogAdapterState {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

ffi_support::implement_into_ffi_by_pointer!(LogAdapterState);
