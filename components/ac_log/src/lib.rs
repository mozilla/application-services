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
//! naive usage of this class in a multithreaded context will be very suboptimal
//! in terms of memory and thread usage.
//!
//! To avoid this, we only call into the JVM from a single thread, which we
//! launch when initializing the logger. This thread just polls a channel
//! listening for log messages, where a log message is an enum (`LogMessage`)
//! that either tells it to log an item, or to stop logging all together.
//!
//! 1. We cannot guarantee the the callback from android lives past when the
//!    android code tells us to stop logging, so in order to be memory safe, we
//!    need to stop logging immediately when this happens. We do this using an
//!    `Arc<AtomicBool>`, used to indicate that we should stop logging.
//!
//! 2. There's no safe way to terminate a thread in Rust (for good reason), so
//!    the background thread must close willingly. To make sure this happens
//!    promptly (e.g. to avoid a case where we're blocked until some thread
//!    somewhere else happens to log something), we need to add something onto
//!    the log channel, hence the existence of `LogMessage::Stop`.
//!
//!    It's important to note that because of point 1, the polling thread may
//!    have to stop prior to getting `LogMessage::Stop`. We do not want to wait
//!    for it to process whatever log messages were sent prior to being told
//!    to stop.
//!
//! Finally, it's worth noting that the log crate is rather inflexable, in that
//! it does not allow users to change loggers after the first initialization. We
//! work around this using our `settable_log` module.

use std::{
    ffi::CString,
    os::raw::c_char,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, SyncSender},
        Arc, Mutex,
    },
    thread,
};

mod settable_log;

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
///
/// and returns 0 if we should close the thread, and 1 otherwise. This is done because
/// attempting to call `close` from within the log callback will deadlock.
pub type LogCallback = extern "C" fn(LogLevel, *const c_char, *const c_char) -> u8;

enum LogMessage {
    Stop,
    Record(LogRecord),
}

pub struct LogAdapterState {
    // Thread handle for the BG thread. We can't drop this without problems so weu32
    // prefix with _ to shut rust up about it being unused.
    handle: Option<std::thread::JoinHandle<()>>,
    stopped: Arc<Mutex<bool>>,
    sender: SyncSender<LogMessage>,
}

pub struct LogSink {
    sender: SyncSender<LogMessage>,
    // Used locally for preventing unnecessary work after the `sender`
    // is closed. Not shared. Not required for correctness.
    disabled: AtomicBool,
}

impl log::Log for LogSink {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // Really this could just be Acquire but whatever
        !self.disabled.load(Ordering::SeqCst)
    }

    fn flush(&self) {}
    fn log(&self, record: &log::Record) {
        // Important: we check stopped before writing, which means
        // it must be set before
        if self.disabled.load(Ordering::SeqCst) {
            // Note: `enabled` is not automatically called.
            return;
        }
        // Either the queue is full, or the receiver is closed.
        // In either case, we want to stop all logging immediately.
        if self
            .sender
            .try_send(LogMessage::Record(record.into()))
            .is_err()
        {
            self.disabled.store(true, Ordering::SeqCst);
        }
    }
}

impl LogAdapterState {
    pub fn init(callback: LogCallback) -> Self {
        // This uses a mutex (instead of an atomic bool) to avoid a race condition
        // where `stopped` gets set by another thread between when we read it and
        // when we call the callback. This way, they'll block.
        let stopped = Arc::new(Mutex::new(false));
        let (message_sender, message_recv) = sync_channel(4096);
        let handle = {
            let stopped = stopped.clone();
            thread::spawn(move || {
                // We stop if we see `Err` (which means the channel got closed,
                // which probably can't happen since the sender owned by the
                // logger will never get dropped), or if we get `LogMessage::Stop`,
                // which means we should stop processing.
                while let Ok(LogMessage::Record(record)) = message_recv.recv() {
                    let LogRecord {
                        tag,
                        level,
                        message,
                    } = record;
                    let tag_ptr = tag
                        .as_ref()
                        .map(|s| s.as_ptr())
                        .unwrap_or_else(std::ptr::null);
                    let msg_ptr = message.as_ptr();

                    let mut stop_guard = stopped.lock().unwrap();
                    if *stop_guard {
                        return;
                    }
                    let keep_going = callback(level, tag_ptr, msg_ptr);
                    if keep_going == 0 {
                        *stop_guard = true;
                        return;
                    }
                }
            })
        };

        let sink = LogSink {
            sender: message_sender.clone(),
            disabled: AtomicBool::new(false),
        };

        settable_log::set_logger(Box::new(sink));
        log::set_max_level(log::LevelFilter::Debug);
        log::info!("ac_log adapter initialized!");
        Self {
            handle: Some(handle),
            stopped,
            sender: message_sender,
        }
    }
}

impl Drop for LogAdapterState {
    fn drop(&mut self) {
        {
            // It would be nice to write a log that says something like
            // "if we deadlock here it's because you tried to close the
            // log adapter from within the log callback", but, well, we
            // can't exactly log anything from here (and even if we could,
            // they'd never see it if they hit that situation)
            let mut stop_guard = self.stopped.lock().unwrap();
            *stop_guard = true;
            // We can ignore a failure here because it means either
            // - The recv is dropped, in which case we don't need to send anything
            // - The recv is completely full, in which case it will see the flag we
            //   wrote into `stop_guard` soon enough anyway.
            let _ = self.sender.try_send(LogMessage::Stop);
        }
        // Wait for the calling thread to stop. This should be relatively
        // quickly unless something terrible has happened.
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
        settable_log::unset_logger();
    })
}

// Used just to allow tests to produce logs.
#[no_mangle]
pub unsafe extern "C" fn ac_log_adapter_test__log_msg(msg: *const c_char) {
    ffi_support::abort_on_panic::call_with_output(|| {
        log::info!("testing: {}", ffi_support::rust_str_from_c(msg));
    });
}
