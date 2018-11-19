/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This crate allows users from the other side of the FFI to hook into Rust's
//! `log` crate, which is used by us and several of our dependencies. The
//! primary use case is providing logs to Android in a way that is more flexible
//! than writing to liblog (which goes to logcat, which cannot be accessed by
//! programs on the device, short of rooting it).
//!
//! Even worse, Rust logs can be emitted by any thread, regardless of whether or
//! not they have an associated JVM thread. JNA's Callback class helps us here,
//! by providing a way for mapping native threads to JVM threads. Unfortunately,
//! naive usage of this class will produce a large number of threads, and
//! exhaust available memory (in theory this might be able to be prevented by
//! using CallbackThreadInitializer, however I wasn't ever able to get that to
//! work).
//!
//! To avoid this, we only call into the JVM from a single thread, which we
//! launch when initializing the logger. Conceptually, this thread just polls a
//! channel listening for log messages. In practice, there are a few
//! complications:
//!
//! 1. We cannot guarantee the the callback from android lives past when the
//!    android code tells us to stop logging, so in order to be memory safe, we
//!    need to stop logging immediately when this happens. We do this using an
//!    `Arc<AtomicBool>`, used to indicate that we should stop logging.
//! 2. There's no safe way to terminate a thread in Rust (for good reason), so
//!    the background thread must close willingly. To make sure this happens
//!    promptly (e.g. to avoid a case where we're blocked until some thread
//!    somewhere else happens to log something), we use a separate channel that
//!    only exists to indicate that the `Arc<AtomicBool>` has changed value.
//!
//! (For future work it might be worth investigate if we can avoid the channel
//! in `2.` by just closing (e.g. dropping) the log message channel's Sender and
//! handling the Err in the background thread. This seems like it would work,
//! but it's possible there are subtle issues).
//!
//! Finally, it's worth noting that the log crate is rather inflexable, in that
//! it does not allow users to change loggers after the first initialization.
//! This inflexibility has leaked into this API, but it's a consequence of
//! `log`'s design, and not of anything fundamental about code calling between
//! Rust and the JVM in this manner.

use crossbeam_channel::Sender;
use std::{
    ffi::CString,
    os::raw::c_char,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

#[derive(Clone, Copy)]
#[repr(i32)]
pub enum LogLevel {
    // Android logger levels
    VERBOSE = 2,
    DEBUG = 3,
    INFO = 4,
    WARN = 5,
    ERROR = 6,
}

impl From<log::Level> for LogLevel {
    fn from(l: log::Level) -> Self {
        match l {
            log::Level::Trace => LogLevel::VERBOSE,
            log::Level::Debug => LogLevel::DEBUG,
            log::Level::Info => LogLevel::INFO,
            log::Level::Warn => LogLevel::WARN,
            log::Level::Error => LogLevel::ERROR,
        }
    }
}

// TODO: use serde to send this to the other thread as bincode or something,
// rather than allocating all these strings for every message.
struct LogRecord {
    level: LogLevel,
    tag: Option<CString>,
    message: CString,
}

fn string_to_cstring_lossy(s: String) -> CString {
    let mut bytes = s.into_bytes();
    for byte in bytes.iter_mut() {
        if *byte == 0 {
            *byte = b'?';
        }
    }
    CString::new(bytes).expect("Bug in string_to_cstring_lossy!")
}

impl<'a, 'b> From<&'b log::Record<'a>> for LogRecord {
    // XXX important! Don't log in this function!
    fn from(r: &'b log::Record<'a>) -> Self {
        let message = format!("{}", r.args());
        Self {
            level: r.level().into(),
            tag: r
                .module_path()
                .and_then(|mp| CString::new(mp.to_owned()).ok()),
            message: string_to_cstring_lossy(message),
        }
    }
}

/// Type of the log callback provided to us by java.
/// Takes the following arguments:
///
/// - Log level (an i32).
/// - Tag: a (nullable) nul terminated c string. Caller must not free this string!
/// - Message: a (non-nullable) nul terminated c string. Caller must not free this string!
pub type LogCallback = extern "C" fn(LogLevel, *const c_char, *const c_char);

pub struct LogAdapterState {
    // Thread handle for the BG thread. We can't drop this without problems so weu32
    // prefix with _ to shut rust up about it being unused.
    handle: Option<std::thread::JoinHandle<()>>,
    stopped: Arc<AtomicBool>,
    done_sender: Sender<()>,
}

pub struct LogSink {
    stopped: Arc<AtomicBool>,
    sender: Sender<LogRecord>,
}

impl log::Log for LogSink {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // Really this could just be Acquire but whatever
        !self.stopped.load(Ordering::SeqCst)
    }

    fn flush(&self) {}
    fn log(&self, record: &log::Record) {
        // Important: we check stopped before writing, which means
        // it must be set before
        if self.stopped.load(Ordering::SeqCst) {
            // Note: `enabled` is not automatically called.
            return;
        }
        // In practice this should never fail, we always set `stopped` before
        // closing the channel. That said, in the future it wouldn't be
        // unreasonable to swallow this error.
        self.sender.send(record.into()).unwrap();
    }
}

impl LogAdapterState {
    pub fn init(callback: LogCallback) -> Self {
        let stopped = Arc::new(AtomicBool::new(false));
        let (record_sender, record_recv) = crossbeam_channel::unbounded();
        // We use a channel to notify the `drain` thread that we changed done,
        // so that we can close it in a timely fashion.
        let (done_sender, done_recv) = crossbeam_channel::bounded(1);
        let handle = {
            let stopped = stopped.clone();
            thread::spawn(move || {
                loop {
                    crossbeam_channel::select! {
                        recv(record_recv) -> record => {
                            if stopped.load(Ordering::SeqCst) {
                                return;
                            }
                            if let Ok(LogRecord { level, tag, message }) = record {
                                let tag_ptr = tag.as_ref()
                                    .map(|s| s.as_ptr())
                                    .unwrap_or_else(std::ptr::null);
                                let msg_ptr = message.as_ptr();
                                callback(level, tag_ptr, msg_ptr);
                            } else {
                                // Channel closed.
                                stopped.store(true, Ordering::SeqCst);
                                return;
                            }
                        },
                        recv(done_recv) -> _ => {
                            return;
                        }
                    };

                    // Could be Acquire
                    if stopped.load(Ordering::SeqCst) {
                        return;
                    }
                }
            })
        };
        let sink = LogSink {
            sender: record_sender,
            stopped: stopped.clone(),
        };

        log::set_max_level(log::LevelFilter::Info);
        log::set_boxed_logger(Box::new(sink)).unwrap();
        log::info!("ac_log adapter initialized!");
        Self {
            handle: Some(handle),
            stopped,
            done_sender,
        }
    }

    pub fn stop(&mut self) {}
}

impl Drop for LogAdapterState {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.done_sender.send(()).unwrap();
        // TODO: can we safely return from this (I suspect the answer is no, and
        // we have to panic and abort higher up...)
        if let Some(h) = self.handle.take() {
            h.join().unwrap();
        }
    }
}

ffi_support::implement_into_ffi_by_pointer!(LogAdapterState);
ffi_support::define_string_destructor!(ac_log_adapter_destroy_string);

#[no_mangle]
pub extern "C" fn ac_log_adapter_create(
    callback: LogCallback,
    out_err: &mut ffi_support::ExternError,
) -> *mut LogAdapterState {
    ffi_support::call_with_output(out_err, || LogAdapterState::init(callback))
}

// Note: keep in sync with LogLevelFilter in kotlin.
fn level_filter_from_i32(level_arg: i32) -> log::LevelFilter {
    match level_arg {
        4 => log::LevelFilter::Debug,
        3 => log::LevelFilter::Info,
        2 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Error,
        // We clamp out of bounds level values.
        n if n <= 0 => log::LevelFilter::Off,
        n if n >= 5 => log::LevelFilter::Trace,
        _ => unreachable!("This is actually exhaustive"),
    }
}

#[no_mangle]
pub extern "C" fn ac_log_adapter_set_max_level(
    _state: &mut LogAdapterState,
    level: i32,
    out_err: &mut ffi_support::ExternError,
) {
    ffi_support::call_with_output(out_err, || log::set_max_level(level_filter_from_i32(level)))
}

// Can't use define_box_destructor because this can panic. TODO: Maybe we should
// keep this around globally (as lazy_static or something) and basically just
// turn it on/off in create/destroy... Might be more reliable?
#[no_mangle]
pub unsafe extern "C" fn ac_log_adapter_destroy(to_destroy: *mut LogAdapterState) {
    ffi_support::abort_on_panic::call_with_output(|| {
        log::set_max_level(log::LevelFilter::Off);
        drop(Box::from_raw(to_destroy));
    })
}

// Used just to allow tests to produce logs.
#[no_mangle]
pub unsafe extern "C" fn ac_log_adapter_test__log_msg(msg: *const c_char) {
    ffi_support::abort_on_panic::call_with_output(|| {
        log::info!("testing: {}", ffi_support::rust_str_from_c(msg));
    });
}
