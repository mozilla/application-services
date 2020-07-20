/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod fxa_creds;
pub mod prompt;

pub use env_logger;

pub fn init_logging_with(s: &str) {
    let noisy = "tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
    let spec = format!("{},{}", s, noisy);
    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", spec));
}

pub fn init_trace_logging() {
    init_logging_with("trace")
}

pub fn init_logging() {
    init_logging_with(if cfg!(debug_assertions) {
        "debug"
    } else {
        "info"
    })
}
