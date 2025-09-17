/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{env, sync::Once};
use tracing_subscriber::{
    filter::{targets::Targets, LevelFilter},
    fmt,
    prelude::*,
};

/// Initialize a logging environment suitable for testing. Logging can be configured using the
/// `RUST_LOG` env variable, using a syntax that more-or-less matches the `env_logger` behavior.
/// See `build_targets_from_env` for the exact behavior.  If not so configured, the filter will
/// default to the `Level::Error` level.
pub fn init_from_env() {
    // This is intended to be equivalent to `env_logger::try_init().ok();`
    // `debug!()` output is seen. We could maybe add logging for `#[tracing::instrument]`?
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(build_targets_from_env(LevelFilter::ERROR, None))
        .init();
}

/// Like `init_for_tests` but uses the specified `level` if logging is not configured in the environment.
pub fn init_from_env_with_level(level: crate::Level) {
    let level_filter = LevelFilter::from_level(level.into());
    // This is intended to be equivalent to `env_logger::try_init().ok();`
    // `debug!()` output is seen. We could maybe add logging for `#[tracing::instrument]`?
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(build_targets_from_env(level_filter, None))
        .init();
}

/// Like `init_for_tests` but uses a default string if logging is not configured in the environment
pub fn init_from_env_with_default(default_env: &str) {
    // This is intended to be equivalent to `env_logger::try_init().ok();`
    // `debug!()` output is seen. We could maybe add logging for `#[tracing::instrument]`?
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(build_targets_from_env(LevelFilter::OFF, Some(default_env)))
        .init();
}

fn build_targets_from_env(default_level: LevelFilter, default_env: Option<&str>) -> Targets {
    let mut targets = Targets::new().with_default(default_level);
    let env = match env::var("RUST_LOG") {
        Ok(env) => env,
        Err(_) => match default_env {
            Some(env) => env.to_string(),
            None => return targets,
        },
    };
    for item in env.split(",") {
        let item = item.trim();
        match item.split_once("=") {
            Some((target, level)) => {
                let level = match try_parse_level(level) {
                    Some(level) => level,
                    None => {
                        println!("Invalid logging level, defaulting to error: {level}");
                        LevelFilter::ERROR
                    }
                };
                targets = targets.with_target(target, level);
            }
            None => match try_parse_level(item) {
                Some(level) => {
                    targets = targets.with_default(level);
                }
                None => {
                    targets = targets.with_target(item, LevelFilter::TRACE);
                }
            },
        }
    }
    targets
}

fn try_parse_level(env_part: &str) -> Option<LevelFilter> {
    match env_part.to_lowercase().as_str() {
        "error" => Some(LevelFilter::ERROR),
        "warn" | "warning" => Some(LevelFilter::WARN),
        "info" => Some(LevelFilter::INFO),
        "debug" => Some(LevelFilter::DEBUG),
        "trace" => Some(LevelFilter::TRACE),
        "off" => Some(LevelFilter::OFF),
        _ => None,
    }
}

static TESTING_SUBSCRIBER: Once = Once::new();

/// Init logging for tests.
///
/// This is the same as `init_from_env`, except it uses a `Once` so that multiple initializations
/// are okay.
pub fn init_for_tests() {
    TESTING_SUBSCRIBER.call_once(init_from_env);
}

/// Init logging for tests.
///
/// This is the same as `init_from_env_with_level`, except it uses a `Once` so that multiple
/// initializations are okay.
pub fn init_for_tests_with_level(level: crate::Level) {
    TESTING_SUBSCRIBER.call_once(|| init_from_env_with_level(level));
}
